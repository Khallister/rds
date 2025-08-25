//! Analysis orchestration and execution logic.
//!
//! This module handles the coordination of dependency analysis, including
//! progress reporting, result processing, and output generation.

use anyhow::Result;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Instant;

use crate::analyzer::DependencyAnalyzer;
use crate::cli::Cli;
use crate::filesystem::FileSystem;
use crate::logger;
use crate::output::{ConsoleOutput, JsonOutput};
use crate::types::AnalysisResult;
use crate::utils::{config, exit_codes};

/// Orchestrates the analysis workflow
pub struct AnalysisRunner;

impl AnalysisRunner {
    pub async fn run_analysis_once(cli: &Cli) -> Result<()> {
        let show_progress = if cli.progress {
            true
        } else {
            atty::is(atty::Stream::Stdout) && std::env::var("CI").is_err()
        };

        logger::debug(&format!("Expanding input files: {:?}", &cli.files));
        let expanded_files = FileSystem::expand_file_inputs(&cli.files, &cli.filter).await?;
        logger::info(&format!("Found {} files to analyze", expanded_files.len()));
        if expanded_files.is_empty() {
            eprintln!("No files found matching the specified criteria");
            return Ok(());
        }

        let mut options = config::create_parse_options_from_cli(cli)?;

        let progress_bar = if show_progress {
            let pb = ProgressBar::new(expanded_files.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("  [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("█▉▊▋▌▍▎▏  "),
            );
            logger::info("Progress bar enabled");
            Some(pb)
        } else {
            None
        };

        if let Some(ref pb) = progress_bar {
            let pb_clone = pb.clone();
            options.progress_callback = Some(Box::new(move |_, msg| {
                pb_clone.set_message(msg.to_string());
                pb_clone.inc(1);
            }));
        }

        logger::debug("Initializing DependencyAnalyzer");
        let mut analyzer = DependencyAnalyzer::new(options)?;

        let start_time = Instant::now();
        if show_progress {
            println!(
                "{}",
                style("🚀 Starting dependency analysis...").bold().green()
            );
            logger::info("Starting analysis (progress shown)");
        }

        logger::debug(&format!(
            "Beginning analysis of {} files",
            expanded_files.len()
        ));
        let (result, num_threads) = analyzer.analyze_files(&expanded_files).await?;
        logger::info("Analysis completed");

        let cache_stats = analyzer.get_cache_stats();

        let duration = start_time.elapsed();

        if let Some(pb) = progress_bar {
            pb.finish_with_message("Complete!");
            logger::info("Progress bar finished");
        }

        println!(
            "🗄️  Cache: {} hits, {} misses, {} files cached, {} tree reuses (hit rate {:.1}%)",
            cache_stats.hits,
            cache_stats.misses,
            cache_stats.cached_files,
            cache_stats.cached_tree_reuses,
            cache_stats.hit_rate
        );

        Self::display_analysis_results(&result, &expanded_files, duration, num_threads, cli)
            .await?;

        if cli.throw && !result.circulars.is_empty() {
            eprintln!(
                "{}",
                style("error: Circular Dependencies found").bold().red()
            );
            eprintln!(
                "  {} circular dependencies detected.",
                result.circulars.len()
            );
            std::process::exit(1);
        }

        if let Some(ref exit_spec) = cli.exit_code {
            exit_codes::handle_exit_codes(exit_spec, &result.circulars)?;
        }

        Ok(())
    }

    /// Display analysis results using appropriate output methods
    async fn display_analysis_results(
        result: &AnalysisResult,
        expanded_files: &[String],
        duration: std::time::Duration,
        num_threads: usize,
        cli: &Cli,
    ) -> Result<()> {
        let console_output = ConsoleOutput::new();

        println!(
            "{} {:.2?})",
            style("✨ Analysis complete!").bold().green(),
            duration
        );
        println!(
            "📊 {} files processed, {} total dependencies in tree",
            expanded_files.len(),
            Self::count_total_dependencies(&result.tree)
        );
        println!(
            "🧵 Analysis used {} threads for parallel processing",
            num_threads
        );
        println!();

        let show_tree = cli.tree;
        let show_circular = cli.circular || (!cli.circular && !cli.tree);

        if show_tree {
            console_output.print_tree(&result.tree, &result.entries);
        }

        if show_circular {
            let console_output = ConsoleOutput::new();
            console_output.print_circular(&result.circulars, cli.take, None);
        }

        if let Some(ref output_path) = cli.output {
            let json_output = JsonOutput::new();
            json_output.write_to_file(result, output_path).await?;
            println!("📄 Results saved to: {}", output_path.display());
        }

        Ok(())
    }

    fn count_total_dependencies(tree: &crate::types::DependencyTree) -> usize {
        tree.values()
            .filter_map(|deps| deps.as_ref())
            .map(|deps| deps.len())
            .sum()
    }
}

#[cfg(test)]
mod tests;
