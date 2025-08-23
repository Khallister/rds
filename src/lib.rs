//! RDS (Rust Dependency Scanner) - Library for dependency analysis
//! 
//! This crate provides functionality for analyzing dependencies in JavaScript,
//! TypeScript, and Vue projects.

pub mod analyzer;
pub mod analysis_runner;
pub mod cache;
pub mod cli;
pub mod config;
pub mod filesystem;
pub mod output;
pub mod parser;
pub mod types;
pub mod utils;
pub mod watch;

// Re-export commonly used types for convenience
pub use analyzer::DependencyAnalyzer;
pub use analysis_runner::AnalysisRunner;
pub use cli::Cli;
pub use types::{AnalysisResult, DependencyTree, ParseOptions};
pub use watch::WatchRunner;
