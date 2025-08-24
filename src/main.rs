//! RDS (Rust Dependency Scanner) - A fast, memory-efficient dependency analyzer
//!
//! This is the main entry point for the RDS application, handling high-level
//! orchestration and delegating to specialized modules.

mod analysis_runner;
mod analyzer;
mod cache;
mod cli;
mod filesystem;
mod output;
mod parser;
mod types;
mod utils;
mod watch;

use anyhow::Result;

use crate::analysis_runner::AnalysisRunner;
use crate::cli::Cli;
use crate::parser::{register_parser, JavaScriptParser, VueParser};
use crate::utils::threading;
use crate::watch::WatchRunner;
use std::sync::Arc;

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
        // register built-in parsers so runtime registry is populated for plugins/tests
        // Each parser advertises the extensions it handles; register them directly.
        register_parser(Arc::new(JavaScriptParser::new()?));
        register_parser(Arc::new(VueParser::new()?));

        AnalysisRunner::run_analysis_once(&cli).await
    }
}
