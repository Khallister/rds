use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
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
        let path_obj = std::path::Path::new(path).to_path_buf();
        let workdir = std::env::current_dir().unwrap_or_default();
        
        // If the path is absolute, try to make it relative to current working directory
        if path_obj.is_absolute() {
            if let Ok(relative) = path_obj.strip_prefix(&workdir) {
                relative.to_string_lossy().replace('\\', "/")
            } else {
                // For paths outside workdir, try to find common parent with workdir
                let path_str = path_obj.to_string_lossy().to_lowercase();
                if path_str.starts_with("c:\\projects\\") || path_str.starts_with("c:/projects/") {
                    // Get the projects-relative portion
                    let after_projects = &path_obj.to_string_lossy()[12..]; // Skip "C:\Projects\"
                    format!("../../{}", after_projects.replace('\\', "/"))
                } else {
                    // Fallback: use absolute path but normalize separators
                    path_obj.to_string_lossy().replace('\\', "/")
                }
            }
        } else {
            // Already relative, just normalize separators
            path_obj.to_string_lossy().replace('\\', "/")
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
        
        let mut tree = DependencyTree::new();
        
        // Get number of available threads
        let num_threads = rayon::current_num_threads();
        
        // Expand glob patterns and collect all files to process
        let mut all_files = Vec::new();
        for entry in entries {
            // Check if this is a direct file path first
            if Path::new(entry).is_file() {
                let absolute_path = if Path::new(entry).is_absolute() {
                    entry.to_string()
                } else {
                    options.context.join(entry).to_string_lossy().to_string()
                };
                all_files.push(absolute_path);
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

        // Multi-threaded recursive dependency parsing
        // Process dependencies in parallel batches, recursively discovering new dependencies
        let mut processed_files = std::collections::HashSet::new();
        let mut files_to_process: Vec<String> = all_files;
        
        use futures::stream::{self, StreamExt};
        let max_concurrent = num_threads.min(32);
        
        while !files_to_process.is_empty() {
            let current_batch: Vec<String> = files_to_process.drain(..).collect();
            let mut new_dependencies = Vec::new();
            
            // Filter out already processed files before parallel processing
            let unprocessed_batch: Vec<String> = current_batch.into_iter()
                .filter(|file_path| {
                    if !processed_files.contains(file_path) {
                        processed_files.insert(file_path.clone());
                        true
                    } else {
                        false
                    }
                })
                .collect();
            
            if unprocessed_batch.is_empty() {
                continue;
            }

            // Check cache first and separate cached vs uncached files
            let mut cached_results = Vec::new();
            let mut files_to_parse = Vec::new();
            
            for file_path in unprocessed_batch {
                if self.cache.is_cached(&file_path).await? {
                    // Use cached dependencies
                    if let Some(cached_deps) = self.cache.get_cached_dependencies(&file_path) {
                        cached_results.push((file_path, Some(cached_deps)));
                    } else {
                        // Cache says it's cached but no deps found - treat as needing parse
                        files_to_parse.push(file_path);
                    }
                } else {
                    // Need to parse this file
                    files_to_parse.push(file_path);
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
                        // Cache the newly parsed dependencies
                        if let Some(ref deps) = dependencies_opt {
                            self.cache.cache_dependencies(&file_path, deps.clone()).await?;
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
        
        // Read file content and parse (no cache in static method)
        let content = fs::read_to_string(file_path).await
            .with_context(|| format!("Failed to read file: {}", file_path))?;
        
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
        
        // Get number of available threads
        let num_threads = rayon::current_num_threads();
        
        // For watch mode with cache enabled, check if we can avoid full analysis
        if options.cache_enabled && changed_files.len() == 1 {
            let changed_file = &changed_files[0];
            
            // If we have a previous analysis, compare the file's dependencies to see if anything meaningful changed
            if let Some((cached_file, cached_tree)) = self.last_analysis_cache.clone() {
                if &cached_file == changed_file {
                    // Parse just the changed file to get its current dependencies (without recursion)
                    let mut temp_tree = DependencyTree::new();
                    self.parse_single_file_deps(changed_file, options, &mut temp_tree).await?;
                    
                    // Use normalized path to get dependencies from temp tree
                    let normalized_changed_file = self.normalize_path_for_storage(changed_file);
                    
                    // Compare current dependencies with cached ones
                    if let Some(Some(new_deps)) = temp_tree.get(&normalized_changed_file) {
                        // Use the same normalization method for cache lookup to ensure consistency
                        let lookup_path = self.normalize_path_for_storage(changed_file);
                        
                        if let Some(Some(old_deps)) = cached_tree.get(&lookup_path) {
                            // Compare dependency requests (import paths)
                            let old_requests: std::collections::HashSet<&str> = 
                                old_deps.iter().map(|d| d.request.as_str()).collect();
                            let new_requests: std::collections::HashSet<&str> = 
                                new_deps.iter().map(|d| d.request.as_str()).collect();
                            
                            if old_requests == new_requests {
                                // Dependencies haven't changed! Return cached tree immediately
                                // This is the key optimization - no full tree rebuild needed
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
        
        // Cache this result for future incremental updates
        if changed_files.len() == 1 {
            self.last_analysis_cache = Some((changed_files[0].clone(), tree.clone()));
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
        let content = tokio::fs::read_to_string(file_path).await?;
        
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
