use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use crate::types::{DependencyTree, ParseOptions, ProgressEvent};
use crate::parser::{JavaScriptParser, VueParser, ModuleResolver};
use crate::cache::{FileCache, CacheStats};

pub struct TreeBuilder {
    js_parser: JavaScriptParser,
    vue_parser: VueParser,
    resolver: ModuleResolver,
    cache: FileCache,
    // Store the last complete analysis result for efficient incremental updates
    last_analysis_cache: Option<(String, DependencyTree)>,
}

impl TreeBuilder {
    /// Convert absolute path to relative path for consistent storage
    fn normalize_path_for_storage(&self, path: &str) -> String {
        use crate::utils::lexical_normalize_abs;
    // no additional imports

        let path_obj = std::path::Path::new(path);
        let workdir = std::env::current_dir().unwrap_or_default();

        // Prefer to canonicalize to a real absolute path; if that fails, fall back to lexical normalization
        let abs = if path_obj.is_absolute() {
            std::fs::canonicalize(path_obj).unwrap_or_else(|_| lexical_normalize_abs(path_obj))
        } else {
            let joined = workdir.join(path_obj);
            std::fs::canonicalize(&joined).unwrap_or_else(|_| lexical_normalize_abs(&joined))
        };

        // Canonicalize workdir similarly so comparisons are consistent
        let workdir_abs = std::fs::canonicalize(&workdir).unwrap_or_else(|_| workdir.clone());

        // Helper to strip Windows long-path device prefix like "\\?\" or "//?/"
        fn strip_device_prefix(s: &str) -> &str {
            // Several representations may appear; check and strip common prefixes
            if let Some(rest) = s.strip_prefix("\\\\?\\") {
                rest
            } else if let Some(rest) = s.strip_prefix("//?/") {
                rest
            } else if let Some(rest) = s.strip_prefix("\\\\?/") {
                rest
            } else {
                s
            }
        }

        let abs_s = abs.to_string_lossy();
        let work_s = workdir_abs.to_string_lossy();
        let abs_stripped = strip_device_prefix(&abs_s).replace('\\', "/");
        let work_stripped = strip_device_prefix(&work_s).replace('\\', "/");

        // If the path is within the working directory, return a relative path
        if abs_stripped.starts_with(&format!("{}", work_stripped)) {
            // Trim leading workdir + optional slash
            let rel = abs_stripped[work_stripped.len()..].trim_start_matches('/').to_string();
            return rel;
        } else {
            // Prefer projects-relative when under C:/Projects
            let lower = abs_stripped.to_lowercase();
            if lower.contains("c:/projects/") {
                if let Some(idx) = lower.find("c:/projects/") {
                    let after = &abs_stripped[idx + "c:/projects/".len()..];
                    return format!("../../{}", after.trim_start_matches('/'));
                }
            }

            // Fallback: use the absolute, stripped, normalized path
            abs_stripped
        }
    }

    pub fn new() -> Result<Self> {
        Ok(Self {
            js_parser: JavaScriptParser::new()?,
            vue_parser: VueParser::new()?,
            resolver: ModuleResolver::new(),
            cache: FileCache::new(true), // Enable cache by default
            last_analysis_cache: None,
        })
    }
    
    pub async fn build_dependency_tree(
        &mut self,
        entries: &[String],
        options: &ParseOptions,
    ) -> Result<(DependencyTree, usize)> {
        // Configure cache based on options
        self.cache.set_enabled(options.cache_enabled);

    // debug logging removed for cleaner watch output
        
        let mut tree = DependencyTree::new();
        
        // Get number of available threads
        let num_threads = rayon::current_num_threads();
        
        // Expand glob patterns and collect all files to process
        let mut all_files = Vec::new();
        for entry in entries {
            // Check if this is a direct file path first
            if Path::new(entry).is_file() {
                let read_path_buf = if Path::new(entry).is_absolute() {
                    PathBuf::from(entry)
                } else {
                    options.context.join(entry)
                };
                all_files.push(read_path_buf.to_string_lossy().to_string());
                continue;
            }
            
            // Otherwise treat as glob pattern
            let paths = glob::glob(entry)
                .with_context(|| format!("Failed to expand glob pattern: {}", entry))?;
            
            for path in paths {
                let path = path?;
                let file_path = path.to_string_lossy().to_string();
                
                // Resolve to absolute path
                let absolute_path = if Path::new(&file_path).is_absolute() {
                    file_path
                } else {
                    options.context.join(&file_path).to_string_lossy().to_string()
                };
                
                all_files.push(absolute_path);
            }
        }

    // debug logging removed for cleaner watch output

        // Multi-threaded recursive dependency parsing
        // Process dependencies in parallel batches, recursively discovering new dependencies
    // Track processed files by their storage-normalized key to avoid
    // duplicate work regardless of whether we see absolute or normalized
    // paths in the pipeline.
    let mut processed_files = std::collections::HashSet::new();
        let mut files_to_process: Vec<String> = all_files;
        
        use futures::stream::{self, StreamExt};
        let max_concurrent = num_threads.min(32);
        
        while !files_to_process.is_empty() {
            let current_batch: Vec<String> = files_to_process.drain(..).collect();
            let mut new_dependencies = Vec::new();
            
            // Filter out already processed files before parallel processing.
            // Normalize each candidate to the storage key and check membership
            // in `processed_files`. We still carry the filesystem path forward
            // in the batch so that file system operations work correctly.
            let mut unprocessed_batch: Vec<String> = Vec::new();
            for file_path in current_batch.into_iter() {
                let normalized = self.normalize_path_for_storage(&file_path);
                if !processed_files.contains(&normalized) {
                    processed_files.insert(normalized);
                    unprocessed_batch.push(file_path);
                }
            }
            
            if unprocessed_batch.is_empty() {
                continue;
            }

            // Check cache first and separate cached vs uncached files
            let mut cached_results = Vec::new();
            let mut files_to_parse = Vec::new();
            
            for file_path in unprocessed_batch {
                // Determine filesystem path to use for metadata/reads. If the
                // path is storage-normalized (starts with ../ or contains no
                // drive), try to resolve it relative to options.context.
                let fs_path = if Path::new(&file_path).is_absolute() {
                    file_path.clone()
                } else {
                    options.context.join(&file_path).to_string_lossy().to_string()
                };

                // Use a storage-normalized key for caching (so keys in the cache
                // match the normalized IDs used in the dependency tree).
                let cache_key = self.normalize_path_for_storage(&fs_path);

                if self.cache.is_cached(&fs_path, &cache_key).await? {
                    // Use cached dependencies
                    if let Some(cached_deps) = self.cache.get_cached_dependencies(&cache_key) {
                        cached_results.push((fs_path.clone(), Some(cached_deps)));
                    } else {
                        // Cache says it's cached but no deps found - treat as needing parse
                        files_to_parse.push(fs_path.clone());
                    }
                } else {
                    // Need to parse this file
                    files_to_parse.push(fs_path.clone());
                }
            }
            
            // Add cached results to tree immediately and queue their dependencies
            for (file_path, deps_opt) in cached_results {
                let normalized_path = self.normalize_path_for_storage(&file_path);
                tree.insert(normalized_path, deps_opt.clone());
                
                // If we got cached dependencies, resolve them and add to next batch
                if let Some(dependencies) = deps_opt {
                    let context = Path::new(&file_path).parent().unwrap_or(Path::new("."));
                    
                    for dep in dependencies {
                        // Resolve the dependency path
                        if let Ok(Some(resolved_path)) = self.resolver.resolve_module(context, &dep.request, &options.extensions).await {
                            let normalized = self.normalize_path_for_storage(&resolved_path);
                            if !processed_files.contains(&normalized) && 
                               !new_dependencies.contains(&normalized) {
                                new_dependencies.push(normalized);
                            }
                        }
                    }
                }
            }
            
            // Skip parallel processing if no files need to be parsed
            if files_to_parse.is_empty() {
                files_to_process = new_dependencies;
                continue;
            }

            // Process uncached files in parallel
            let js_parser = &self.js_parser;
            let vue_parser = &self.vue_parser;
            let mut file_results = stream::iter(files_to_parse)
                .map(|file_path| {
                    Box::pin(async move {
                        Self::parse_file_static(&file_path, options, js_parser, vue_parser).await
                            .map_err(|e| (file_path.clone(), e))
                    })
                })
                .buffer_unordered(max_concurrent);
            
            // Collect results and cache them
            while let Some(result) = file_results.next().await {
                match result {
                    Ok((file_path, dependencies_opt)) => {
                        // `file_path` here is a filesystem path; compute a
                        // storage-normalized cache key to store the dependencies
                        // under a deterministic ID used by the tree.
                        if let Some(ref deps) = dependencies_opt {
                            let cache_key = self.normalize_path_for_storage(&file_path);
                            self.cache.cache_dependencies(&file_path, &cache_key, deps.clone()).await?;
                        }

                        let normalized_path = self.normalize_path_for_storage(&file_path);
                        tree.insert(normalized_path, dependencies_opt.clone());
                        
                        // If we got dependencies, resolve them and add to next batch
                        if let Some(dependencies) = dependencies_opt {
                            let context = Path::new(&file_path).parent().unwrap_or(Path::new("."));
                            
                            for dep in dependencies {
                                // Resolve the dependency path
                                if let Ok(Some(resolved_path)) = self.resolver.resolve_module(context, &dep.request, &options.extensions).await {
                                    let normalized = self.normalize_path_for_storage(&resolved_path);
                                    if !processed_files.contains(&normalized) && 
                                       !new_dependencies.contains(&normalized) {
                                        new_dependencies.push(normalized);
                                    }
                                }
                            }
                        }
                    },
                    Err((file_path, error)) => {
                        return Err(anyhow::anyhow!("Failed to parse file {}: {}", file_path, error));
                    }
                }
            }
            
            // Add new dependencies to process in next iteration
            files_to_process = new_dependencies;
        }
        
        // Use resolve_dependencies to ensure all dependency IDs are resolved
        self.resolve_dependencies(&mut tree, options).await?;

        // Apply context shortening if specified
        if options.context != PathBuf::from(".") {
            let shortened_tree = self.shorten_tree(&options.context, tree)?;
            return Ok((shortened_tree, num_threads));
        }

        Ok((tree, num_threads))
    }
    
    /// Static file parsing method for parallel processing (no cache access)
    async fn parse_file_static(
        file_path: &str,
        options: &ParseOptions,
        js_parser: &JavaScriptParser,
        vue_parser: &VueParser,
    ) -> Result<(String, Option<Vec<crate::types::Dependency>>)> {
        // Check include/exclude patterns
        if !options.include.is_match(file_path) || options.exclude.is_match(file_path) {
            return Ok((file_path.to_string(), None));
        }
        
        // Check if this is a parseable file type
        let path = Path::new(file_path);
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        let is_js_like = options.js_extensions.iter()
            .any(|ext| ext.trim_start_matches('.') == extension);
        let is_vue = options.vue_extensions.iter()
            .any(|ext| ext.trim_start_matches('.') == extension);
        
        if !is_js_like && !is_vue {
            return Ok((file_path.to_string(), Some(Vec::new())));
        }
        
        // Call progress callback
        if let Some(ref callback) = options.progress_callback {
            callback(ProgressEvent::Start, file_path);
        }
        
        // Resolve file_path to an absolute path for reading. Some callers pass
        // storage-normalized paths (e.g. "../../project/...") which should be
        // resolved relative to the analysis context.
        // Interpret storage-normalized paths that were produced by
        // `normalize_path_for_storage`, e.g. "../../project/...", as paths
        // relative to the parent of the workdir. This maps sibling projects
        // correctly (e.g. C:\Projects\rds + "../../lingo-prototype" ->
        // C:\Projects\lingo-prototype).
        let read_path_buf = if Path::new(file_path).is_absolute() {
            PathBuf::from(file_path)
        } else if file_path.starts_with("../../") || file_path.starts_with("..\\..\\") {
            let cwd = std::env::current_dir().unwrap_or_default();
            let rest = &file_path[6..]; // strip ../../ or ..\..\ (length 6)
            let parent = cwd.parent().unwrap_or(&cwd);
            parent.join(rest)
        } else {
            options.context.join(file_path)
        };

        // Read file content and parse (no cache in static method)
        let content = crate::utils::read_file_text_async(&read_path_buf).await?;
        
        // Parse dependencies
        let dependencies = if is_vue {
            vue_parser.parse_file(file_path, &content)?
        } else {
            js_parser.parse_file(file_path, &content)?
        };
        
        // Call progress callback
        if let Some(ref callback) = options.progress_callback {
            callback(ProgressEvent::End, file_path);
        }
        
        Ok((file_path.to_string(), Some(dependencies)))
    }
    
    /// Get cache statistics
    pub fn get_cache_stats(&self) -> CacheStats {
        self.cache.get_stats()
    }
    
    /// Get incremental cache statistics (resets counters)
    pub fn get_incremental_cache_stats(&mut self) -> CacheStats {
        self.cache.get_incremental_stats()
    }
    
    /// Incremental dependency tree build - only analyzes changed files
    pub async fn build_dependency_tree_incremental(
        &mut self,
        changed_files: &[String], 
        options: &ParseOptions
    ) -> Result<(DependencyTree, usize)> {
        // Configure cache based on options
        self.cache.set_enabled(options.cache_enabled);

    // debug logging removed for cleaner watch output
        
        // Get number of available threads
        let num_threads = rayon::current_num_threads();
        
        // For watch mode with cache enabled, check if we can avoid full analysis
        if options.cache_enabled && changed_files.len() == 1 {
            let changed_file = &changed_files[0];

            // single-file changed (watch mode)
            
            // If we have a previous analysis, compare the file's dependencies to see if anything meaningful changed
            if let Some((cached_file, cached_tree)) = self.last_analysis_cache.clone() {
                // Normalize the changed file key for comparison with cached key
                let normalized_changed_file = self.normalize_path_for_storage(changed_file);
                if cached_file == normalized_changed_file {
                    // changed_file matches cached key -> attempting single-file parse comparison
                    // Parse just the changed file to get its current dependencies (without recursion)
                    let mut temp_tree = DependencyTree::new();
                    self.parse_single_file_deps(changed_file, options, &mut temp_tree).await?;

                    // Compare current dependencies with cached ones
                    if let Some(Some(new_deps)) = temp_tree.get(&normalized_changed_file) {
                        if let Some(Some(old_deps)) = cached_tree.get(&normalized_changed_file) {
                            // Compare dependency requests (import paths)
                            let old_requests: std::collections::HashSet<&str> = 
                                old_deps.iter().map(|d| d.request.as_str()).collect();
                            let new_requests: std::collections::HashSet<&str> = 
                                new_deps.iter().map(|d| d.request.as_str()).collect();

                            if old_requests == new_requests {
                                // Dependencies haven't changed! Return cached tree immediately
                                // Count this reuse so the UI can report it
                                self.cache.incr_cached_tree_reuse();
                                return Ok((cached_tree, num_threads));
                            }
                        }
                    }
                } else {
                    // File path mismatch, fall through to full analysis
                }
            } else {
                // No cached analysis available, fall through to full analysis
            }
        }
        
        // Dependencies have changed or no cache available - do full analysis
    let (mut tree, threads) = self.build_dependency_tree(changed_files, options).await?;
        
        // Use resolve_dependencies to ensure all dependency IDs are resolved
        self.resolve_dependencies(&mut tree, options).await?;
        
        // Cache this result for future incremental updates (store normalized key)
        if changed_files.len() == 1 {
            let key = self.normalize_path_for_storage(&changed_files[0]);
            self.last_analysis_cache = Some((key, tree.clone()));
        }
        
        Ok((tree, threads))
    }
    
    /// Parse a single file to get its direct dependencies without recursion (for cache comparison)
    async fn parse_single_file_deps(
        &mut self,
        file_path: &str,
        options: &ParseOptions,
        tree: &mut DependencyTree,
    ) -> Result<()> {
        // Check include/exclude patterns
        if !options.include.is_match(file_path) || options.exclude.is_match(file_path) {
            let normalized_path = self.normalize_path_for_storage(file_path);
            tree.insert(normalized_path, None);
            return Ok(());
        }
        
        // Check if this is a parseable file type
        let path = Path::new(file_path);
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        let is_js_like = options.js_extensions.iter()
            .any(|ext| ext.trim_start_matches('.') == extension);
        let is_vue = options.vue_extensions.iter()
            .any(|ext| ext.trim_start_matches('.') == extension);
            
        if !is_js_like && !is_vue {
            let normalized_path = self.normalize_path_for_storage(file_path);
            tree.insert(normalized_path, Some(Vec::new()));
            return Ok(());
        }
        
        // Always re-parse the file to get its current dependencies (no cache)
        // This is used for incremental analysis to detect dependency changes
        let read_path_buf = if Path::new(file_path).is_absolute() {
            PathBuf::from(file_path)
        } else if file_path.starts_with("../../") || file_path.starts_with("..\\..\\") {
            let cwd = std::env::current_dir().unwrap_or_default();
            let rest = &file_path[6..];
            let parent = cwd.parent().unwrap_or(&cwd);
            parent.join(rest)
        } else {
            options.context.join(file_path)
        };

        let content = crate::utils::read_file_text_async(&read_path_buf).await?;
        
        let dependencies = if is_vue {
            self.vue_parser.parse_file(file_path, &content)?
        } else {
            self.js_parser.parse_file(file_path, &content)?
        };
        
        // Add to tree with normalized path (no recursion, no caching)
        let normalized_path = self.normalize_path_for_storage(file_path);
        tree.insert(normalized_path, Some(dependencies));
        
        Ok(())
    }

    async fn resolve_dependencies(
        &self,
        tree: &mut DependencyTree,
        options: &ParseOptions,
    ) -> Result<()> {
        // Since ModuleResolver is not Send/Sync, we'll do sequential processing
        // but still report the thread count for consistency
        
        // Apply all resolutions to the tree
        let mut all_resolutions = Vec::new();
        
        for (file_id, deps_opt) in tree.iter() {
            if let Some(dependencies) = deps_opt {
                let context = Path::new(file_id).parent().unwrap_or(Path::new("."));
                
                for dep in dependencies {
                    if let Ok(Some(resolved)) = self.resolver.resolve_module(context, &dep.request, &options.extensions).await {
                        let normalized = self.normalize_path_for_storage(&resolved);
                        all_resolutions.push((file_id.clone(), dep.request.clone(), normalized));
                    }
                }
            }
        }
        
        // Apply all resolutions back to the tree
        for (file_id, request, resolved_id) in all_resolutions {
            if let Some(Some(dependencies)) = tree.get_mut(&file_id) {
                for dep in dependencies {
                    if dep.request == request {
                        dep.id = Some(resolved_id.clone());
                        break;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn shorten_tree(&self, context: &Path, tree: DependencyTree) -> Result<DependencyTree> {
        let mut shortened = DependencyTree::new();
        
        for (key, deps_opt) in tree {
            let short_key = Path::new(&key)
                .strip_prefix(context)
                .unwrap_or(Path::new(&key))
                .to_string_lossy()
                .replace('\\', "/");
            
            let shortened_deps = if let Some(dependencies) = deps_opt {
                Some(dependencies.into_iter().map(|mut dep| {
                    dep.issuer = short_key.clone();
                    if let Some(ref id) = dep.id {
                        let normalized_id = Path::new(id)
                            .strip_prefix(context)
                            .unwrap_or(Path::new(id))
                            .to_string_lossy()
                            .replace('\\', "/");
                        dep.id = Some(normalized_id);
                    }
                    dep
                }).collect())
            } else {
                None
            };
            
            shortened.insert(short_key, shortened_deps);
        }
        
        Ok(shortened)
    }
}
