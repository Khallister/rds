//! Utility functions and helpers used throughout the RDS application.

use notify::Event;
use std::path::{Path, PathBuf, Component};
use anyhow::Context;
use std::env;
use tokio::fs as tokio_fs;
use std::fs as std_fs;

pub fn lexical_normalize_abs(path: &Path) -> PathBuf {
    let mut base = if path.is_absolute() {
        PathBuf::new()
    } else {
        env::current_dir().unwrap_or_default()
    };

    for comp in path.components() {
        match comp {
            Component::Prefix(p) => base.push(p.as_os_str()),
            Component::RootDir => base.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => { let _ = base.pop(); }
            Component::Normal(os) => base.push(os),
        }
    }

    base
}

pub fn extract_relevant_file_changes(event: &Event, _watched_files: &[String]) -> Vec<String> {
    let mut changed_files = Vec::new();
    
    match event.kind {
        notify::EventKind::Create(_) | 
        notify::EventKind::Modify(_) | 
        notify::EventKind::Remove(_) => {
            for path in &event.paths {
                if let Some(path_str) = path.to_str() {
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

fn is_relevant_file_change(path: &Path) -> bool {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        matches!(extension, "js" | "jsx" | "ts" | "tsx" | "mjs" | "json" | "vue")
    } else {
        false
    }
}

pub mod config {
    use crate::types::{ParseOptions, SkipDynamicImports};
    use crate::cli::{Cli, SkipDynamicImportsArg};
    use anyhow::Result;
    
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
    
        pub fn configure_thread_pool(threads: Option<usize>) -> Result<()> {
        if let Some(thread_count) = threads {
                        rayon::ThreadPoolBuilder::new()
                .num_threads(thread_count)
                .build_global()
                .map_err(|e| anyhow::anyhow!("Failed to configure thread pool: {}", e))?;
        }
        Ok(())
    }
}

pub mod exit_codes {
    use anyhow::Result;
    
        pub fn handle_exit_codes(spec: &str, circulars: &[Vec<String>]) -> Result<()> {
        let mut exit_code = 0;
        
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

pub async fn read_file_text_async(path: &Path) -> anyhow::Result<String> {
        let cwd = env::current_dir().unwrap_or_default();

                  let attempted_fs = tokio::fs::canonicalize(path).await;
    let attempted = match attempted_fs {
        Ok(p) => p,
        Err(_) => lexical_normalize_abs(path),
    };

    tokio_fs::read_to_string(&attempted)
        .await
        .with_context(|| format!("Failed to read file: {} (attempted: {}) from cwd: {}", path.display(), attempted.display(), cwd.display()))
}

pub fn read_file_text_sync(path: &Path) -> anyhow::Result<String> {
    let cwd = env::current_dir().unwrap_or_default();

    let attempted = match std_fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => lexical_normalize_abs(path),
    };

    std_fs::read_to_string(&attempted)
        .with_context(|| format!("Failed to read file: {} (attempted: {}) from cwd: {}", path.display(), attempted.display(), cwd.display()))
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
