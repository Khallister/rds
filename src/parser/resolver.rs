use anyhow::{Context, Result};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};
use tokio::fs;

pub struct ModuleResolver {
    builtin_modules: std::collections::HashSet<String>,
}

impl ModuleResolver {
    pub fn new() -> Self {
        let mut builtin_modules = std::collections::HashSet::new();
        // Add Node.js built-in modules
        for module in &[
            "assert", "buffer", "child_process", "cluster", "crypto", "dgram",
            "dns", "domain", "events", "fs", "http", "https", "module", "net",
            "os", "path", "punycode", "querystring", "readline", "stream",
            "string_decoder", "timers", "tls", "tty", "url", "util", "vm", "zlib"
        ] {
            builtin_modules.insert(module.to_string());
        }
        
        Self { builtin_modules }
    }
    
    // Normalize path separators to be consistent with the current platform
    fn normalize_path(path: &str) -> String {
        let normalized = if cfg!(windows) {
            path.replace('/', "\\")
        } else {
            path.replace('\\', "/")
        };
        
        // Manual normalization to resolve .. and . components
        let separator = if cfg!(windows) { "\\" } else { "/" };
        let parts: Vec<&str> = normalized.split(separator).filter(|p| !p.is_empty()).collect();
        let mut result = Vec::new();
        
        for part in parts {
            match part {
                ".." => {
                    if !result.is_empty() && result.last() != Some(&"..") {
                        result.pop();
                    } else {
                        result.push(part);
                    }
                }
                "." => {
                    // Skip current directory references
                }
                "" => {
                    // Skip empty parts
                }
                _ => {
                    result.push(part);
                }
            }
        }
        
        // Reconstruct path, preserving leading separator for absolute paths
        let result_path = result.join(separator);
        if normalized.starts_with(separator) {
            format!("{}{}", separator, result_path)
        } else {
            result_path
        }
    }
    
    // Normalize a PathBuf to use consistent separators
    fn normalize_pathbuf(path: &Path) -> String {
        Self::normalize_path(&path.to_string_lossy())
    }
    
    pub async fn resolve_module<P: AsRef<Path>>(
        &self,
        context: P,
        request: &str,
        extensions: &[String],
    ) -> Result<Option<String>> {
        let context = context.as_ref();
        
        // Skip built-in modules
        if self.builtin_modules.contains(request) {
            return Ok(Some(request.to_string()));
        }
        
        // Try TypeScript path mapping first
        if let Some(resolved) = self.resolve_ts_alias(context, request, extensions).await? {
            return Ok(Some(resolved));
        }
        
        // Absolute path
        if Path::new(request).is_absolute() {
            return self.append_suffix(request, extensions).await;
        }
        
        // Relative path
        if request.starts_with('.') {
            let resolved = context.join(request);
            let normalized_path = Self::normalize_pathbuf(&resolved);
            return self.append_suffix(&normalized_path, extensions).await;
        }

        // Node module
        let result = self.resolve_node_module(context, request, extensions).await?;
        // Ensure the final result is also normalized
        if let Some(path) = result {
            Ok(Some(Self::normalize_path(&path)))
        } else {
            Ok(None)
        }
    }
    
    fn append_suffix<'a>(
        &'a self,
        request: &'a str,
        extensions: &'a [String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>> {
        Box::pin(async move {
            // First, try the path as-is (in case it already has an extension)
            if let Ok(metadata) = fs::metadata(request).await {
                if metadata.is_file() {
                    return Ok(Some(Self::normalize_path(request)));
                }
            }
            
            // Try with each extension
            for ext in extensions {
                let path_with_ext = format!("{}{}", request, ext);
                if let Ok(metadata) = fs::metadata(&path_with_ext).await {
                    if metadata.is_file() {
                        return Ok(Some(Self::normalize_path(&path_with_ext)));
                    }
                }
            }
            
            // Try as directory with index file
            if let Ok(metadata) = fs::metadata(request).await {
                if metadata.is_dir() {
                    let index_path = Path::new(request).join("index");
                    let normalized_index = Self::normalize_pathbuf(&index_path);
                    return Box::pin(self.append_suffix(&normalized_index, extensions)).await;
                }
            }
            
            Ok(None)
        })
    }
    
    async fn resolve_node_module<P: AsRef<Path>>(
        &self,
        context: P,
        request: &str,
        extensions: &[String],
    ) -> Result<Option<String>> {
        let mut current = context.as_ref().to_path_buf();
        
        loop {
            let node_modules = current.join("node_modules").join(request);
            
            // Try to resolve package.json
            let package_json_path = node_modules.join("package.json");
            if let Ok(package_content) = fs::read_to_string(&package_json_path).await {
                if let Ok(package_json) = serde_json::from_str::<serde_json::Value>(&package_content) {
                    let main_field = package_json.get("module")
                        .or_else(|| package_json.get("main"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("index.js");
                    
                    let main_path = node_modules.join(main_field);
                    if let Some(resolved) = self.append_suffix(&main_path.to_string_lossy(), extensions).await? {
                        return Ok(Some(resolved));
                    }
                }
            }
            
            // Try direct file resolution
            if let Some(resolved) = self.append_suffix(&node_modules.to_string_lossy(), extensions).await? {
                return Ok(Some(resolved));
            }
            
            // Move up to parent directory
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => break,
            }
        }
        
        Ok(None)
    }
    
    async fn resolve_ts_alias(
        &self,
        context: &Path,
        request: &str,
        extensions: &[String],
    ) -> Result<Option<String>> {
        
        // Skip scoped npm packages (e.g., @fortawesome/vue-fontawesome)
        // These start with @ but contain a slash after the organization name
        // Don't skip TypeScript aliases like @/path which start with @/
        if request.starts_with('@') && !request.starts_with("@/") && request.chars().skip(1).any(|c| c == '/') {
            // Check if this looks like a scoped npm package: @org/package
            let parts: Vec<&str> = request.splitn(3, '/').collect();
            if parts.len() >= 2 && parts[0].starts_with('@') && !parts[1].is_empty() {
                // This is likely a scoped npm package like @fortawesome/vue-fontawesome
                // Don't try to resolve it as a TypeScript alias
                return Ok(None);
            }
        }
        
        // Look for tsconfig.json starting from context directory
        let mut current_dir = context.to_path_buf();
        
        loop {
            let tsconfig_path = current_dir.join("tsconfig.json");
            
            if let Ok(content) = fs::read_to_string(&tsconfig_path).await {
                if let Ok(tsconfig) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(paths) = tsconfig
                        .get("compilerOptions")
                        .and_then(|c| c.get("paths"))
                        .and_then(|p| p.as_object()) 
                    {
                        // Try to match request against path patterns
                        for (pattern, targets) in paths {
                            if let Some(resolved) = self.match_ts_path_pattern(
                                &tsconfig_path.parent().unwrap(),
                                request,
                                pattern,
                                targets,
                                extensions
                            ).await? {
                                return Ok(Some(resolved));
                            }
                        }
                    }
                }
            }
            
            // Move up to parent directory
            match current_dir.parent() {
                Some(parent) => current_dir = parent.to_path_buf(),
                None => break,
            }
        }
        
        Ok(None)
    }
    
    async fn match_ts_path_pattern(
        &self,
        base_dir: &Path,
        request: &str,
        pattern: &str,
        targets: &serde_json::Value,
        extensions: &[String],
    ) -> Result<Option<String>> {
        
        // Handle wildcard patterns like "@/*" -> ["./src/*"]
        if pattern.ends_with("/*") {
            let pattern_prefix = &pattern[..pattern.len()-2]; // Remove "/*"
            
            if request.starts_with(pattern_prefix) {
                let remaining = &request[pattern_prefix.len()..]; // Get the part after prefix (including /)
                
                if let Some(target_array) = targets.as_array() {
                    for target in target_array {
                        if let Some(target_str) = target.as_str() {
                            if target_str.ends_with("/*") {
                                let target_base = &target_str[..target_str.len()-2]; // Remove "/*"
                                
                                // Handle relative paths starting with "./"
                                let target_path = if target_base.starts_with("./") {
                                    base_dir.join(&target_base[2..])
                                } else if target_base.starts_with("/") {
                                    PathBuf::from(target_base)
                                } else {
                                    base_dir.join(target_base)
                                };
                                
                                // Append the remaining path and normalize
                                let resolved_path = if remaining.is_empty() || remaining == "/" {
                                    target_path
                                } else if remaining.starts_with('/') {
                                    target_path.join(&remaining[1..])
                                } else {
                                    target_path.join(remaining)
                                };
                                
                                let normalized_resolved = Self::normalize_pathbuf(&resolved_path);
                                
                                if let Some(result) = self.append_suffix(
                                    &normalized_resolved, 
                                    extensions
                                ).await? {
                                    return Ok(Some(result));
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Handle exact matches like "@/utils" -> ["./src/utils"]
            if request == pattern {
                if let Some(target_array) = targets.as_array() {
                    for target in target_array {
                        if let Some(target_str) = target.as_str() {
                            let target_path = if target_str.starts_with("./") {
                                base_dir.join(&target_str[2..])
                            } else if target_str.starts_with("/") {
                                PathBuf::from(target_str)
                            } else {
                                base_dir.join(target_str)
                            };
                            
                            let normalized_target = Self::normalize_pathbuf(&target_path);
                            
                            if let Some(result) = self.append_suffix(
                                &normalized_target, 
                                extensions
                            ).await? {
                                return Ok(Some(result));
                            }
                        }
                    }
                }
            }
        }
        
        Ok(None)
    }
}
