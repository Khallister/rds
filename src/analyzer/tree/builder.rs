use crate::analyzer::tree::reverse_index::ReverseIndex;
use crate::cache::{CacheStats, FileCache};
use crate::parser::ModuleResolver;
use crate::types::{DependencyTree, ParseOptions};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::expand;
use super::parse;
use super::partition;
use super::resolve;
use crate::logger;

pub struct TreeBuilder {
    resolver: ModuleResolver,
    cache: FileCache,
    last_analysis_cache: Option<(String, DependencyTree)>,
    // Reverse index: encapsulated in a helper
    reverse_index: ReverseIndex,
    // Last full tree built (in the same key-space as reverse_index).
    // Used as a fallback when reverse_index is empty to compute dependents.
    last_full_tree: Option<DependencyTree>,
}

impl TreeBuilder {
    pub fn new() -> Result<Self> {
        // Parsers are selected dynamically per-file in the parse module.
        Ok(Self {
            resolver: ModuleResolver::new(),
            cache: FileCache::new(true),
            last_analysis_cache: None,
            reverse_index: ReverseIndex::new(),
            last_full_tree: None,
        })
    }

    pub async fn build_dependency_tree(
        &mut self,
        entries: &[String],
        options: &ParseOptions,
    ) -> Result<(DependencyTree, usize)> {
        // Perform a full build, then shorten keys for index and populate
        // reverse_index and last_full_tree in the key-space used for callers.
        let (tree, num_threads) = self.build_tree_core(entries, options).await?;

        // Prepare the tree that will be returned to callers. Shorten when needed.
        let tree_for_index: DependencyTree = if options.context != PathBuf::from(".") {
            resolve::shorten_tree(&options.context, tree.clone())?
        } else {
            tree.clone()
        };

        // Build reverse index from the tree_for_index so incremental runs can
        // quickly compute affected dependents.
        self.reverse_index = ReverseIndex::from_tree(&tree_for_index);
        self.last_full_tree = Some(tree_for_index.clone());

        if options.context != PathBuf::from(".") {
            return Ok((tree_for_index, num_threads));
        }

        Ok((tree, num_threads))
    }

    async fn build_tree_core(
        &mut self,
        entries: &[String],
        options: &ParseOptions,
    ) -> Result<(DependencyTree, usize)> {
        let build_start = std::time::Instant::now();
        self.cache.set_enabled(options.cache_enabled);
        let mut tree = DependencyTree::new();
        let num_threads = rayon::current_num_threads();
        let all_files = expand::expand_entries(entries, options)?;
        let mut processed_files = HashSet::new();
        let mut files_to_process: Vec<String> = all_files;
        let max_concurrent = num_threads.min(32);

        let mut loop_count = 0usize;
        while !files_to_process.is_empty() {
            let loop_start = std::time::Instant::now();
            logger::debug(&format!(
                "[While Loop]: Files to process: {}, Processed files: {}",
                files_to_process.len(),
                processed_files.len()
            ));
            let current_batch: Vec<String> = files_to_process.drain(..).collect();
            let mut new_dependencies = Vec::new();

            // Normalize and filter out already processed files
            let unprocessed_batch =
                Self::normalize_and_filter_batch(current_batch, &mut processed_files).await?;

            if unprocessed_batch.is_empty() {
                continue;
            }

            let (cached_results, files_to_parse) =
                partition::partition_cached(&mut self.cache, unprocessed_batch, options).await?;

            // Handle cached results: insert into tree and collect discovered deps
            self.handle_cached_results(
                cached_results,
                &mut tree,
                &mut processed_files,
                &mut new_dependencies,
                options,
            )
            .await?;

            if files_to_parse.is_empty() {
                files_to_process = new_dependencies;
                loop_count += 1;
                let loop_elapsed = loop_start.elapsed();
                crate::logger::info(&format!(
                    "build_tree_core: loop {} completed, processed_files={}, next_batch_size={}, loop_time={}ms",
                    loop_count,
                    processed_files.len(),
                    files_to_process.len(),
                    loop_elapsed.as_millis()
                ));
                continue;
            }

            logger::debug(&format!(
                "Parsing batch: {} files (max_concurrent={})",
                files_to_parse.len(),
                max_concurrent
            ));

            // Parse and process the batch (delegated to parser helpers)
            self.parse_and_process_batch(
                files_to_parse,
                options,
                max_concurrent,
                &mut tree,
                &mut processed_files,
                &mut new_dependencies,
            )
            .await?;

            files_to_process = new_dependencies;
        }

        let total_build_elapsed = build_start.elapsed();
        crate::logger::info(&format!(
            "build_tree_core: build completed before resolve, files_in_tree={}, loops={}, total_build_time={}ms",
            tree.len(),
            loop_count,
            total_build_elapsed.as_millis()
        ));

        resolve::resolve_dependencies(&self.resolver, &mut tree, options).await?;

        Ok((tree, num_threads))
    }

    pub fn get_cache_stats(&self) -> CacheStats {
        self.cache.get_stats()
    }

    // Debug helper: return list of reverse_index keys (for tests/diagnostics)
    #[allow(dead_code)]
    pub fn get_reverse_index_keys(&self) -> Vec<String> {
        self.reverse_index.keys()
    }

    #[allow(dead_code)]
    pub fn get_reverse_index_parents(&self, id: &str) -> Option<Vec<String>> {
        self.reverse_index.get_parents(id)
    }

    pub fn get_incremental_cache_stats(&mut self) -> CacheStats {
        self.cache.get_incremental_stats()
    }

    /// Invalidate caches related to the provided paths. This will remove
    /// entries from the file cache and ask the resolver to invalidate
    /// resolver-specific caches.
    pub async fn invalidate_caches(&mut self, paths: &[String]) {
        // Normalize incoming paths where appropriate and remove from file cache
        self.cache.invalidate_paths(paths);

        // Ask resolver to invalidate its caches
        self.resolver.invalidate_paths(paths).await;
    }

    /// Clear all caches (file cache + resolver caches)
    pub async fn clear_all_caches(&mut self) {
        self.cache.clear();
        self.resolver.clear_all_caches().await;
    }

    #[allow(dead_code)]
    pub(crate) fn cache_mut(&mut self) -> &mut FileCache {
        &mut self.cache
    }

    pub async fn build_dependency_tree_incremental(
        &mut self,
        changed_files: &[String],
        options: &ParseOptions,
    ) -> Result<(DependencyTree, usize)> {
        self.cache.set_enabled(options.cache_enabled);

        let num_threads = rayon::current_num_threads();

        if options.cache_enabled && changed_files.len() == 1 {
            let changed_file = &changed_files[0];

            if let Some((cached_file, cached_tree)) = self.last_analysis_cache.clone() {
                let normalized_changed_file =
                    crate::utils::path::normalize_path_for_storage(changed_file)?;
                if cached_file == normalized_changed_file {
                    let mut temp_tree = DependencyTree::new();
                    parse::parse_single_file_deps(
                        &mut self.cache,
                        changed_file,
                        options,
                        &mut temp_tree,
                    )
                    .await?;

                    if let Some(Some(new_deps)) = temp_tree.get(&normalized_changed_file) {
                        if let Some(Some(old_deps)) = cached_tree.get(&normalized_changed_file) {
                            let old_requests: std::collections::HashSet<&str> =
                                old_deps.iter().map(|d| d.request.as_str()).collect();
                            let new_requests: std::collections::HashSet<&str> =
                                new_deps.iter().map(|d| d.request.as_str()).collect();

                            if old_requests == new_requests {
                                self.cache.incr_cached_tree_reuse();
                                return Ok((cached_tree, num_threads));
                            }
                        }
                    }
                } else {
                }
            } else {
            }
        }

        // Compute affected set using reverse index; if index is empty or affected set
        // is too large, fall back to full analysis.
        let affected = self.reverse_index.compute_affected_set(changed_files).await;
        let affected_len = affected.len();

        if std::env::var("RDS_WATCH_DEBUG").is_ok() {
            crate::logger::info(&format!(
                "[TreeBuilder] changed_files={:?}, affected_len={}, affected_sample={:?}",
                changed_files,
                affected_len,
                affected.iter().take(10).cloned().collect::<Vec<_>>()
            ));
        }
        let max_affected_threshold = 500usize;

        // Use last_analysis_cache size as a rough total file estimate when available
        let total_files_estimate = if let Some((_, ref last_tree)) = self.last_analysis_cache {
            last_tree.len()
        } else {
            0usize
        };
        let relative_threshold = if total_files_estimate > 0 {
            (total_files_estimate as f64 * 0.25) as usize
        } else {
            max_affected_threshold
        };

        if self.reverse_index.is_empty()
            || affected_len == 0
            || affected_len > max_affected_threshold
            || affected_len > relative_threshold
        {
            // If reverse_index is empty or produced no affected files, but we
            // have a last_full_tree, attempt a best-effort transitive scan
            // over that tree to compute dependents before doing a full rebuild.
            if affected_len == 0 {
                if let Some(ref last_tree) = self.last_full_tree {
                    let mut queue: Vec<String> = Vec::new();
                    let mut affected_fallback: HashSet<String> = HashSet::new();

                    for f in changed_files {
                        if let Ok(nf) =
                            crate::utils::path::normalize_path_for_storage_cached(f).await
                        {
                            queue.push(nf.clone());
                            if let Some(base) = std::path::Path::new(&nf)
                                .file_name()
                                .and_then(|s| s.to_str())
                            {
                                queue.push(base.to_string());
                            }
                        }
                    }

                    while let Some(curr) = queue.pop() {
                        if affected_fallback.contains(&curr) {
                            continue;
                        }
                        affected_fallback.insert(curr.clone());

                        for (file, deps_opt) in last_tree.iter() {
                            if affected_fallback.contains(file) {
                                continue;
                            }
                            if let Some(deps) = deps_opt {
                                for dep in deps {
                                    if let Some(ref id) = dep.id {
                                        if id == &curr {
                                            queue.push(file.clone());
                                            break;
                                        }
                                    } else if dep.request.ends_with(&curr) {
                                        queue.push(file.clone());
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if !affected_fallback.is_empty() {
                        // Map any basename-only entries in affected_fallback to the
                        // full paths that exist in last_tree. This avoids passing
                        // bare filenames (e.g. "Header.vue") into the full
                        // build which would cause reads relative to cwd.
                        let mut mapped: std::collections::HashSet<String> =
                            std::collections::HashSet::new();
                        for (file, _deps_opt) in last_tree.iter() {
                            if affected_fallback.contains(file) {
                                mapped.insert(file.clone());
                                continue;
                            }
                            if let Some(base) = std::path::Path::new(file)
                                .file_name()
                                .and_then(|s| s.to_str())
                            {
                                if affected_fallback.contains(base) {
                                    mapped.insert(file.clone());
                                }
                            }
                        }

                        let affected_vec: Vec<String> = mapped.into_iter().collect();
                        let (mut tree, threads) =
                            self.build_dependency_tree(&affected_vec, options).await?;
                        resolve::resolve_dependencies(&self.resolver, &mut tree, options).await?;

                        if changed_files.len() == 1 {
                            let key = crate::utils::path::normalize_path_for_storage_cached(
                                &changed_files[0],
                            )
                            .await?;
                            self.last_analysis_cache = Some((key, tree.clone()));
                        }

                        return Ok((tree, threads));
                    }
                }
            }

            // Fall back to full analysis
            let (mut tree, threads) = self.build_dependency_tree(changed_files, options).await?;
            resolve::resolve_dependencies(&self.resolver, &mut tree, options).await?;

            if changed_files.len() == 1 {
                let key = crate::utils::path::normalize_path_for_storage(&changed_files[0])?;
                self.last_analysis_cache = Some((key, tree.clone()));
            }

            return Ok((tree, threads));
        }

        // Limit analysis to affected set
        let affected_vec: Vec<String> = affected.into_iter().collect();

        // Build the subset tree using the same core builder.
        let (partial_tree, threads) = self.build_tree_core(&affected_vec, options).await?;
        // partial_tree already has resolved ids because build_tree_core calls
        // resolve_dependencies at the end.

        // Merge partial_tree into last_full_tree (if present) and update reverse_index
        if let Some(ref mut last_tree) = self.last_full_tree {
            // Merge partial_tree into last_tree incrementally.
            // For each updated issuer key, remove its previous reverse_index
            // mappings, replace the entry, and insert the new mappings.
            // Merge partial into last_tree and update reverse index in one step
            self.reverse_index
                .merge_partial_into_full(&partial_tree, last_tree);
            // After merging updated entries, prune any stale reverse_index refs
            if let Some(ref lt) = self.last_full_tree {
                self.reverse_index.prune(lt);
            }
        } else {
            // No full tree yet; set partial as last_full_tree for future runs.
            self.last_full_tree = Some(partial_tree.clone());

            // Build reverse_index from partial
            let mut idx: HashMap<String, HashSet<String>> = HashMap::new();
            for (k, deps_opt) in partial_tree.iter() {
                if let Some(deps) = deps_opt {
                    for dep in deps {
                        if let Some(ref id) = dep.id {
                            idx.entry(id.clone())
                                .or_insert_with(HashSet::new)
                                .insert(k.clone());
                        }
                    }
                }
            }
            self.reverse_index = ReverseIndex::from_tree(&partial_tree);
            // Ensure no stale entries are left (defensive for future merges)
            if let Some(ref lt) = self.last_full_tree {
                self.reverse_index.prune(lt);
            }
        }

        if changed_files.len() == 1 {
            let key = crate::utils::path::normalize_path_for_storage(&changed_files[0])?;
            self.last_analysis_cache = Some((key, partial_tree.clone()));
        }

        // If we have a last_full_tree (we merged partials into it above),
        // return that merged snapshot so callers (and circular detection)
        // operate on the full up-to-date tree rather than only the partial.
        if let Some(ref last_tree) = self.last_full_tree {
            return Ok((last_tree.clone(), threads));
        }

        Ok((partial_tree, threads))
    }

    // compute_affected_set moved into ReverseIndex

    // reverse-index pruning is handled by ReverseIndex::prune
}

impl TreeBuilder {
    async fn normalize_and_filter_batch(
        batch: Vec<String>,
        processed_files: &mut HashSet<String>,
    ) -> Result<Vec<String>> {
        let mut unprocessed: Vec<String> = Vec::new();
        for file_path in batch.into_iter() {
            let normalized =
                crate::utils::path::normalize_path_for_storage_cached(&file_path).await?;
            if !processed_files.contains(&normalized) {
                processed_files.insert(normalized);
                unprocessed.push(file_path);
            }
        }
        Ok(unprocessed)
    }

    async fn handle_cached_results(
        &mut self,
        cached_results: Vec<(String, Option<Vec<crate::types::Dependency>>)>,
        tree: &mut DependencyTree,
        processed_files: &mut HashSet<String>,
        new_dependencies: &mut Vec<String>,
        options: &ParseOptions,
    ) -> Result<()> {
        for (file_path, deps_opt) in cached_results {
            let normalized_path =
                crate::utils::path::normalize_path_for_storage_cached(&file_path).await?;
            tree.insert(normalized_path, deps_opt.clone());

            if let Some(dependencies) = deps_opt {
                let context = Path::new(&file_path).parent().unwrap_or(Path::new("."));
                for dep in dependencies {
                    if let Some(id) = dep.id {
                        if !processed_files.contains(&id) && !new_dependencies.contains(&id) {
                            new_dependencies.push(id);
                        }
                    } else if let Ok(Some(resolved_path)) = self
                        .resolver
                        .resolve_module(context, &dep.request, &options.extensions)
                        .await
                    {
                        let normalized =
                            crate::utils::path::normalize_path_for_storage_cached(&resolved_path)
                                .await?;
                        if !processed_files.contains(&normalized)
                            && !new_dependencies.contains(&normalized)
                        {
                            new_dependencies.push(normalized);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn parse_and_process_batch(
        &mut self,
        files_to_parse: Vec<String>,
        options: &ParseOptions,
        max_concurrent: usize,
        tree: &mut DependencyTree,
        processed_files: &mut HashSet<String>,
        new_dependencies: &mut Vec<String>,
    ) -> Result<()> {
        let parsed_results =
            parse::parse_files_batch(files_to_parse, options, max_concurrent).await;
        logger::debug("Batch parse completed");

        parse::process_parsed_results(
            &mut self.cache,
            &self.resolver,
            parsed_results,
            tree,
            processed_files,
            new_dependencies,
            options,
        )
        .await?;

        Ok(())
    }
}
