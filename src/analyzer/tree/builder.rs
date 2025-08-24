use crate::cache::{CacheStats, FileCache};
use crate::parser::ModuleResolver;
use crate::types::{DependencyTree, ParseOptions};
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::expand;
use super::parse;
use super::partition;
use super::resolve;

pub struct TreeBuilder {
    resolver: ModuleResolver,
    cache: FileCache,
    last_analysis_cache: Option<(String, DependencyTree)>,
}

impl TreeBuilder {
    pub fn new() -> Result<Self> {
        // Parsers are selected dynamically per-file in the parse module.
        Ok(Self {
            resolver: ModuleResolver::new(),
            cache: FileCache::new(true),
            last_analysis_cache: None,
        })
    }

    pub async fn build_dependency_tree(
        &mut self,
        entries: &[String],
        options: &ParseOptions,
    ) -> Result<(DependencyTree, usize)> {
        self.cache.set_enabled(options.cache_enabled);
        let mut tree = DependencyTree::new();
        let num_threads = rayon::current_num_threads();
        let all_files = expand::expand_entries(entries, options)?;
        let mut processed_files = HashSet::new();
        let mut files_to_process: Vec<String> = all_files;
        let max_concurrent = num_threads.min(32);

        while !files_to_process.is_empty() {
            let current_batch: Vec<String> = files_to_process.drain(..).collect();
            let mut new_dependencies = Vec::new();

            let mut unprocessed_batch: Vec<String> = Vec::new();
            for file_path in current_batch.into_iter() {
                let normalized = crate::utils::path::normalize_path_for_storage(&file_path)?;
                if !processed_files.contains(&normalized) {
                    processed_files.insert(normalized);
                    unprocessed_batch.push(file_path);
                }
            }

            if unprocessed_batch.is_empty() {
                continue;
            }

            let (cached_results, files_to_parse) =
                partition::partition_cached(&mut self.cache, unprocessed_batch, options).await?;

            for (file_path, deps_opt) in cached_results {
                let normalized_path = crate::utils::path::normalize_path_for_storage(&file_path)?;
                tree.insert(normalized_path, deps_opt.clone());

                if let Some(dependencies) = deps_opt {
                    let context = Path::new(&file_path).parent().unwrap_or(Path::new("."));
                    for dep in dependencies {
                        if let Ok(Some(resolved_path)) = self
                            .resolver
                            .resolve_module(context, &dep.request, &options.extensions)
                            .await
                        {
                            let normalized =
                                crate::utils::path::normalize_path_for_storage(&resolved_path)?;
                            if !processed_files.contains(&normalized)
                                && !new_dependencies.contains(&normalized)
                            {
                                new_dependencies.push(normalized);
                            }
                        }
                    }
                }
            }

            if files_to_parse.is_empty() {
                files_to_process = new_dependencies;
                continue;
            }

            let parsed_results =
                parse::parse_files_batch(files_to_parse, options, max_concurrent).await;

            parse::process_parsed_results(
                &mut self.cache,
                &self.resolver,
                parsed_results,
                &mut tree,
                &mut processed_files,
                &mut new_dependencies,
                options,
            )
            .await?;

            files_to_process = new_dependencies;
        }

        resolve::resolve_dependencies(&self.resolver, &mut tree, options).await?;

        if options.context != PathBuf::from(".") {
            let shortened_tree = resolve::shorten_tree(&options.context, tree)?;
            return Ok((shortened_tree, num_threads));
        }

        Ok((tree, num_threads))
    }

    pub fn get_cache_stats(&self) -> CacheStats {
        self.cache.get_stats()
    }

    pub fn get_incremental_cache_stats(&mut self) -> CacheStats {
        self.cache.get_incremental_stats()
    }

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

        let (mut tree, threads) = self.build_dependency_tree(changed_files, options).await?;
        resolve::resolve_dependencies(&self.resolver, &mut tree, options).await?;

        if changed_files.len() == 1 {
            let key = crate::utils::path::normalize_path_for_storage(&changed_files[0])?;
            self.last_analysis_cache = Some((key, tree.clone()));
        }

        Ok((tree, threads))
    }
}
