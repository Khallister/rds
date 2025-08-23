//! Command-line interface definitions and parsing logic for RDS.
//! 
//! This module contains the CLI argument definitions and related parsing logic,
//! separated from the main application logic for better testability and maintainability.

use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Command-line arguments for RDS dependency analyzer
#[derive(Parser, Clone, Debug)]
#[command(name = "rds")]
#[command(about = "A memory-efficient dependency analyzer for JavaScript, TypeScript, and Vue projects")]
#[command(version)]
pub struct Cli {
    /// Input files or directories to analyze
    #[arg(required = true, help = "Input files or directories to analyze")]
    pub files: Vec<String>,
    
    /// Base directory for resolving relative paths
    #[arg(long, help = "Base directory for resolving relative paths")]
    pub context: Option<PathBuf>,
    
    /// File extensions to analyze (comma-separated)
    #[arg(long, alias = "ext", default_value = ".ts,.tsx,.mjs,.js,.jsx,.json,.vue", 
          help = "File extensions to analyze (comma-separated)")]
    pub extensions: String,
    
    /// JavaScript file extensions (comma-separated)
    #[arg(long, default_value = ".ts,.tsx,.mjs,.js,.jsx", 
          help = "JavaScript file extensions (comma-separated)")]
    pub js: String,
    
    /// Filter files by extension when scanning directories
    #[arg(long, help = "Filter files by extension when scanning directories (e.g., 'js,ts,vue')")]
    pub filter: Option<String>,
    
    /// Regex pattern for files to include
    #[arg(long, default_value = ".*", help = "Regex pattern for files to include")]
    pub include: String,
    
    /// Regex pattern for files/directories to exclude
    #[arg(long, default_value = "node_modules|\\.git|\\.svn|\\.hg|coverage|dist|build|out|\\.next|\\.nuxt",
          help = "Regex pattern for files/directories to exclude")]
    pub exclude: String,
    
    /// Output file path for JSON results
    #[arg(short = 'o', long, help = "Output file path for JSON results")]
    pub output: Option<PathBuf>,
    
    /// Show dependency tree visualization
    #[arg(long, action = clap::ArgAction::SetTrue, help = "Show dependency tree visualization")]
    pub tree: bool,
    
    /// Detect and show circular dependencies
    #[arg(long, action = clap::ArgAction::SetTrue, 
          help = "Detect and show circular dependencies")]
    pub circular: bool,
    
    /// Show warning messages during analysis
    #[arg(long, action = clap::ArgAction::SetTrue, help = "Show warning messages during analysis")]
    pub warning: bool,
    
    /// Enable verbose logging output
    #[arg(long, action = clap::ArgAction::SetTrue, help = "Enable verbose logging output")]
    pub log: bool,
    
    /// Exit with code 1 if circular dependencies are found
    #[arg(long, action = clap::ArgAction::SetTrue, help = "Exit with code 1 if circular dependencies are found")]
    pub throw: bool,
    
    /// Path to tsconfig.json for TypeScript path resolution
    #[arg(long, help = "Path to tsconfig.json for TypeScript path resolution")]
    pub tsconfig: Option<PathBuf>,
    
    /// Enable code transformations during parsing
    #[arg(short = 'T', long, help = "Enable code transformations during parsing")]
    pub transform: bool,
    
    /// Custom exit codes (format: 'case:code,case:code')
    #[arg(long, help = "Custom exit codes (format: 'case:code,case:code')")]
    pub exit_code: Option<String>,
    
    /// Show progress bar (auto-detected if not specified)
    #[arg(long, help = "Show progress bar (auto-detected if not specified)")]
    pub progress: Option<bool>,
    
    /// Pattern to detect unused files from
    #[arg(long, help = "Pattern to detect unused files from")]
    pub detect_unused_files_from: Option<String>,
    
    /// Skip dynamic imports in tree or circular analysis
    #[arg(long, value_enum, help = "Skip dynamic imports in tree or circular analysis")]
    pub skip_dynamic_imports: Option<SkipDynamicImportsArg>,
    
    /// Maximum number of circular dependencies to find before stopping
    #[arg(long, help = "Maximum number of circular dependencies to find before stopping")]
    pub take: Option<usize>,
    
    /// Watch mode: monitor files for changes and re-run analysis
    #[arg(short = 'W', long, action = clap::ArgAction::SetTrue, 
          help = "Watch mode: monitor files for changes and re-run analysis")]
    pub watch: bool,
    
    /// Enable file caching to speed up repeated analysis
    #[arg(long, action = clap::ArgAction::SetTrue, 
          help = "Enable file caching to speed up repeated analysis")]
    pub cache: bool,
    
    /// Disable file caching (override default)
    #[arg(long, action = clap::ArgAction::SetTrue, 
          help = "Disable file caching (override default)")]
    pub no_cache: bool,
    
    /// Number of threads to use for parallel processing
    #[arg(long, help = "Number of threads to use for parallel processing")]
    pub threads: Option<usize>,
}

/// Skip dynamic imports argument values
#[derive(Clone, ValueEnum, Debug)]
pub enum SkipDynamicImportsArg {
    Tree,
    Circular,
}

impl Cli {
    /// Parse command-line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }
    
    /// Validate CLI arguments and return errors for invalid combinations
    pub fn validate(&self) -> Result<(), String> {
        if self.cache && self.no_cache {
            return Err("Cannot specify both --cache and --no-cache".to_string());
        }
        
        if let Some(threads) = self.threads {
            if threads == 0 {
                return Err("Number of threads must be greater than 0".to_string());
            }
        }
        
        if let Some(take) = self.take {
            if take == 0 {
                return Err("Take value must be greater than 0".to_string());
            }
        }
        
        Ok(())
    }
    
    /// Get the effective cache setting based on CLI flags
    pub fn effective_cache_setting(&self) -> bool {
        if self.no_cache {
            false
        } else if self.cache {
            true
        } else {
            // Default: enable cache in watch mode, disable otherwise
            self.watch
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_validation_conflicting_cache_flags() {
        let mut cli = Cli::parse_args();
        cli.cache = true;
        cli.no_cache = true;
        
        assert!(cli.validate().is_err());
    }
    
    #[test]
    fn test_cli_validation_zero_threads() {
        let mut cli = Cli::parse_args();
        cli.threads = Some(0);
        
        assert!(cli.validate().is_err());
    }
    
    #[test]
    fn test_effective_cache_setting() {
        let mut cli = Cli::parse_args();
        
        // Default: cache disabled
        assert!(!cli.effective_cache_setting());
        
        // Watch mode: cache enabled by default
        cli.watch = true;
        assert!(cli.effective_cache_setting());
        
        // Explicit no-cache overrides watch mode
        cli.no_cache = true;
        assert!(!cli.effective_cache_setting());
        
        // Explicit cache overrides no-cache
        cli.cache = true;
        cli.no_cache = false;
        assert!(cli.effective_cache_setting());
    }
}
