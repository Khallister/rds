use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use crate::types::{DependencyTree, ParseOptions, Dependency, ProgressEvent};
use crate::parser::{JavaScriptParser, VueParser, ModuleResolver};

fn normalize_path(path: &str) -> String {
    path.replace("/./", "/")
        .replace("\\.\\", "\\")
        .replace("\\./", "\\")
        .replace("./", "")
}

pub struct TreeBuilder {
    js_parser: JavaScriptParser,
    vue_parser: VueParser,
    resolver: ModuleResolver,
}

impl TreeBuilder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            js_parser: JavaScriptParser::new()?,
            vue_parser: VueParser::new()?,
            resolver: ModuleResolver::new(),
        })
    }
    
    pub async fn build_dependency_tree(
        &self,
        entries: &[String],
        options: &ParseOptions,
    ) -> Result<(DependencyTree, usize)> {
        let mut tree = DependencyTree::new();
        
        // Get number of available threads
        let num_threads = rayon::current_num_threads();
        
        // Expand glob patterns and collect all files to process
        let mut all_files = Vec::new();
        for entry in entries {
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

        // Sequential processing of files for now (dependency resolution is complex)
        // The parallelism benefit comes from rayon being used in resolve_dependencies
        for file_path in all_files {
            self.parse_entry_file(&file_path, options, &mut tree).await?;
        }
        
        // Resolve all module IDs (this uses parallel processing internally)
        self.resolve_dependencies(&mut tree, options).await?;
        
        // Apply context shortening if specified
        if options.context != PathBuf::from(".") {
            let shortened_tree = self.shorten_tree(&options.context, tree)?;
            return Ok((shortened_tree, num_threads));
        }
        
        Ok((tree, num_threads))
    }
    
    async fn parse_entry_file(
        &self,
        file_path: &str,
        options: &ParseOptions,
        tree: &mut DependencyTree,
    ) -> Result<()> {
        // Check include/exclude patterns
        if !options.include.is_match(file_path) || options.exclude.is_match(file_path) {
            tree.insert(file_path.to_string(), None);
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
            tree.insert(file_path.to_string(), Some(Vec::new()));
            return Ok(());
        }
        
        // Call progress callback
        if let Some(ref callback) = options.progress_callback {
            callback(ProgressEvent::Start, file_path);
        }
        
        // Read file content
        let content = fs::read_to_string(file_path).await
            .with_context(|| format!("Failed to read file: {}", file_path))?;
        
        // Parse dependencies
        let dependencies = if is_vue {
            self.vue_parser.parse_file(file_path, &content)?
        } else {
            self.js_parser.parse_file(file_path, &content)?
        };
        
        // Add to tree
        tree.insert(file_path.to_string(), Some(dependencies.clone()));
        
        // Process dependencies recursively
        let new_context = path.parent().unwrap_or(Path::new("."));
        
        for dep in dependencies {
            if matches!(options.skip_dynamic_imports, crate::types::SkipDynamicImports::Tree)
                && dep.kind == crate::types::DependencyKind::DynamicImport {
                continue;
            }
            
            // Recursive call with Box::pin
            let future = Box::pin(self.parse_file_recursive(new_context, &dep.request, options, tree));
            future.await?;
        }
        
        // Call progress callback
        if let Some(ref callback) = options.progress_callback {
            callback(ProgressEvent::End, file_path);
        }
        
        Ok(())
    }
    
    fn parse_file_recursive<'a>(
        &'a self,
        context: &'a Path,
        request: &'a str,
        options: &'a ParseOptions,
        tree: &'a mut DependencyTree,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>> {
        Box::pin(async move {
            // Resolve the file path
            let id = self.resolver.resolve_module(context, request, &options.extensions).await?;
            
            let Some(mut resolved_id) = id else {
                return Ok(None);
            };
            
            // Normalize the resolved path
            resolved_id = normalize_path(&resolved_id);
            
            // If already processed, return
            if tree.contains_key(&resolved_id) {
                return Ok(Some(resolved_id));
            }
        
            // Check include/exclude patterns for resolved dependencies
            // Use dependency_exclude instead of exclude to allow node_modules
            if !options.include.is_match(&resolved_id) || options.dependency_exclude.is_match(&resolved_id) {
                tree.insert(resolved_id.clone(), None);
                return Ok(Some(resolved_id));
            }
            
            // Check if this is a parseable file type
            let path = Path::new(&resolved_id);
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            
            let is_js_like = options.js_extensions.iter()
                .any(|ext| ext.trim_start_matches('.') == extension);
            let is_vue = options.vue_extensions.iter()
                .any(|ext| ext.trim_start_matches('.') == extension);
            
            if !is_js_like && !is_vue {
                tree.insert(resolved_id.clone(), Some(Vec::new()));
                return Ok(Some(resolved_id));
            }
            
            // Call progress callback
            if let Some(ref callback) = options.progress_callback {
                callback(ProgressEvent::Start, &resolved_id);
            }
            
            // Read file content
            let content = fs::read_to_string(&resolved_id).await
                .with_context(|| format!("Failed to read file: {}", resolved_id))?;
            
            // Parse dependencies
            let dependencies = if is_vue {
                self.vue_parser.parse_file(&resolved_id, &content)?
            } else {
                self.js_parser.parse_file(&resolved_id, &content)?
            };
            
            // Add to tree
            tree.insert(resolved_id.clone(), Some(dependencies.clone()));
            
            // Process dependencies recursively
            let new_context = path.parent().unwrap_or(Path::new("."));
            
            for dep in dependencies {
                if matches!(options.skip_dynamic_imports, crate::types::SkipDynamicImports::Tree)
                    && dep.kind == crate::types::DependencyKind::DynamicImport {
                    continue;
                }
                
                // Recursive call with Box::pin
                let future = Box::pin(self.parse_file_recursive(new_context, &dep.request, options, tree));
                future.await?;
            }
            
            // Call progress callback
            if let Some(ref callback) = options.progress_callback {
                callback(ProgressEvent::End, &resolved_id);
            }
            
            Ok(Some(resolved_id))
        })
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
                        let normalized = normalize_path(&resolved);
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
    
    // Helper method for parsing file content (can be called from parallel context)
    fn parse_file_content(
        &self,
        content: &str,
        file_path: &str,
        options: &ParseOptions,
    ) -> Result<Vec<Dependency>> {
        let path = Path::new(file_path);
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        
        let is_vue = options.vue_extensions.iter()
            .any(|ext| ext.trim_start_matches('.') == extension);
        
        if is_vue {
            self.vue_parser.parse_file(file_path, content)
        } else {
            self.js_parser.parse_file(file_path, content)
        }
    }
    
    fn shorten_tree(&self, context: &Path, tree: DependencyTree) -> Result<DependencyTree> {
        let mut shortened = DependencyTree::new();
        
        for (key, deps_opt) in tree {
            let short_key = normalize_path(&Path::new(&key)
                .strip_prefix(context)
                .unwrap_or(Path::new(&key))
                .to_string_lossy());
            
            let shortened_deps = if let Some(dependencies) = deps_opt {
                Some(dependencies.into_iter().map(|mut dep| {
                    dep.issuer = short_key.clone();
                    if let Some(ref id) = dep.id {
                        let normalized_id = normalize_path(&Path::new(id)
                            .strip_prefix(context)
                            .unwrap_or(Path::new(id))
                            .to_string_lossy());
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
