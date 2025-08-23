//! Utility functions and helpers used throughout the RDS application.

use notify::Event;
use std::path::Path;

/// Extract relevant file changes from a file system event
pub fn extract_relevant_file_changes(event: &Event, _watched_files: &[String]) -> Vec<String> {
    let mut changed_files = Vec::new();
    
    match event.kind {
        notify::EventKind::Create(_) | 
        notify::EventKind::Modify(_) | 
        notify::EventKind::Remove(_) => {
            for path in &event.paths {
                if let Some(path_str) = path.to_str() {
                    // Only include supported file types
                    if is_relevant_file_change(path) {
                        changed_files.push(path_str.to_string());
                    }
                }
            }
        }
        _ => {}
    }
    
    changed_files
}

/// Check if a file change is relevant for dependency analysis
fn is_relevant_file_change(path: &Path) -> bool {
    // Check if it's a supported file type
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        matches!(extension, "js" | "jsx" | "ts" | "tsx" | "mjs" | "json" | "vue")
    } else {
        false
    }
}

/// Configuration utilities
pub mod config {
    use crate::types::{ParseOptions, SkipDynamicImports};
    use crate::cli::{Cli, SkipDynamicImportsArg};
    use anyhow::Result;
    
    /// Create ParseOptions from CLI arguments
    pub fn create_parse_options_from_cli(cli: &Cli) -> Result<ParseOptions> {
        let mut options = ParseOptions::default();
        
        if let Some(context) = &cli.context {
            options.context = context.clone();
        }
        
        options.extensions = cli.extensions.split(',').map(|s| s.to_string()).collect();
        options.js_extensions = cli.js.split(',').map(|s| s.to_string()).collect();
        options.include = regex::Regex::new(&cli.include)?;
        options.exclude = regex::Regex::new(&cli.exclude)?;
        options.dependency_exclude = regex::Regex::new(r"node_modules|\.git|\.svn|\.hg")?;
        options.tsconfig = cli.tsconfig.clone();
        options.transform = cli.transform;
        options.take = cli.take;
        options.skip_dynamic_imports = match cli.skip_dynamic_imports {
            Some(SkipDynamicImportsArg::Tree) => SkipDynamicImports::Tree,
            Some(SkipDynamicImportsArg::Circular) => SkipDynamicImports::Circular,
            None => SkipDynamicImports::Never,
        };
        options.cache_enabled = cli.effective_cache_setting();
        
        Ok(options)
    }
}

/// Thread pool configuration utilities
pub mod threading {
    use anyhow::Result;
    
    /// Configure the tokio runtime thread pool if specified
    pub fn configure_thread_pool(threads: Option<usize>) -> Result<()> {
        if let Some(thread_count) = threads {
            // Configure rayon thread pool for CPU-intensive work
            rayon::ThreadPoolBuilder::new()
                .num_threads(thread_count)
                .build_global()
                .map_err(|e| anyhow::anyhow!("Failed to configure thread pool: {}", e))?;
        }
        Ok(())
    }
}

/// Exit code handling utilities  
pub mod exit_codes {
    use anyhow::Result;
    
    /// Handle custom exit codes based on analysis results
    pub fn handle_exit_codes(spec: &str, circulars: &[Vec<String>]) -> Result<()> {
        let mut exit_code = 0;
        
        // Parse exit code specification
        for part in spec.split(',') {
            let parts: Vec<&str> = part.split(':').collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!("Invalid exit code format: {}", part));
            }
            
            let case = parts[0];
            let code = parts[1].parse::<i32>()
                .map_err(|_| anyhow::anyhow!("Invalid exit code number: {}", parts[1]))?;
            
            match case {
                "circular" => {
                    if !circulars.is_empty() {
                        exit_code = code;
                    }
                }
                _ => {
                    return Err(anyhow::anyhow!("Unknown exit code case: {}", case));
                }
            }
        }
        
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_is_relevant_file_change() {
        assert!(is_relevant_file_change(&PathBuf::from("test.js")));
        assert!(is_relevant_file_change(&PathBuf::from("test.ts")));
        assert!(is_relevant_file_change(&PathBuf::from("test.vue")));
        assert!(!is_relevant_file_change(&PathBuf::from("test.txt")));
        assert!(!is_relevant_file_change(&PathBuf::from("README.md")));
    }
    
    #[test]
    fn test_handle_exit_codes_no_circulars() {
        let result = exit_codes::handle_exit_codes("circular:1", &[]);
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_handle_exit_codes_invalid_format() {
        let result = exit_codes::handle_exit_codes("invalid_format", &[]);
        assert!(result.is_err());
    }
}
