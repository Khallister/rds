//! RDS (Rust Dependency Scanner) - A fast, memory-efficient dependency analyzer
//! 
//! This is the main entry point for the RDS application, handling high-level
//! orchestration and delegating to specialized modules.

mod analyzer;
mod analysis_runner;
mod cache;
mod cli;
mod config;
mod filesystem;
mod output;
mod parser;
mod types;
mod utils;
mod watch;

use anyhow::Result;

use crate::analysis_runner::AnalysisRunner;
use crate::cli::Cli;
use crate::utils::threading;
use crate::watch::WatchRunner;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse_args();
    
        if let Err(e) = cli.validate() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    
        if let Err(e) = threading::configure_thread_pool(cli.threads) {
        eprintln!("Warning: {}", e);
    }
    
    if cli.watch {
        WatchRunner::run_watch_mode(&cli).await
    } else {
        AnalysisRunner::run_analysis_once(&cli).await
    }
}
