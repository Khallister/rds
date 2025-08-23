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
use crate::output::{ConsoleOutput, JsonOutput};
use crate::types::AnalysisResult;
use crate::utils::{config, exit_codes};

/// Orchestrates the analysis workflow
pub struct AnalysisRunner;

impl AnalysisRunner {
    /// Run a complete analysis once (non-watch mode)
    pub async fn run_analysis_once(cli: &Cli) -> Result<()> {
        let show_progress = cli.progress.unwrap_or_else(|| {
            atty::is(atty::Stream::Stdout) && std::env::var("CI").is_err()
        });

        // Expand directories and apply filters
        let expanded_files = FileSystem::expand_file_inputs(&cli.files, &cli.filter).await?;
        if expanded_files.is_empty() {
            eprintln!("No files found matching the specified criteria");
            return Ok(());
        }

        // Create parse options from CLI
        let mut options = config::create_parse_options_from_cli(cli)?;
        
        // Set up progress callback if needed
        let progress_bar = if show_progress {
            let pb = ProgressBar::new(expanded_files.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("  [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("█▉▊▋▌▍▎▏  ")
            );
            Some(pb)
        } else {
            None
        };
        
        // Set progress callback if we have a progress bar
        if let Some(ref pb) = progress_bar {
            let pb_clone = pb.clone();
            options.progress_callback = Some(Box::new(move |_, msg| {
                pb_clone.set_message(msg.to_string());
                pb_clone.inc(1);
            }));
        }

        // Initialize analyzer
        let mut analyzer = DependencyAnalyzer::new(options)?;

        // Run analysis with timing
        let start_time = Instant::now();
        if show_progress {
            println!("{}", style("🚀 Starting dependency analysis...").bold().green());
        }

    let (result, num_threads) = analyzer.analyze_files(&expanded_files).await?;

    // Retrieve cache statistics from the analyzer (uses TreeBuilder/FileCache)
    let cache_stats = analyzer.get_cache_stats();

    let duration = start_time.elapsed();
        
        // Finish progress bar
        if let Some(pb) = progress_bar {
            pb.finish_with_message("Complete!");
        }

        // Print cache statistics summary
        println!("🗄️  Cache: {} hits, {} misses, {} files cached (hit rate {:.1}%)",
            cache_stats.hits, cache_stats.misses, cache_stats.cached_files, cache_stats.hit_rate);

        // Display results
        Self::display_analysis_results(&result, &expanded_files, duration, num_threads, cli).await?;
        
        // Handle exit codes
        if cli.throw && !result.circulars.is_empty() {
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
        
        // Print summary statistics
        println!("{} {:.2?})", 
            style("✨ Analysis complete!").bold().green(),
            duration
        );
        println!("📊 {} files processed, {} total dependencies in tree",
            expanded_files.len(),
            Self::count_total_dependencies(&result.tree)
        );
        println!("🧵 Analysis used {} threads for parallel processing", num_threads);
        println!();

        // Show appropriate analysis results based on CLI flags
        let show_tree = cli.tree || (!cli.circular && !cli.tree);
        let show_circular = cli.circular || (!cli.circular && !cli.tree);

        if show_tree {
            console_output.print_tree(&result.tree, &result.entries);
        }

        if show_circular {
            Self::print_circular_dependencies(&result.circulars, cli.take);
        }

        // Save to JSON if requested
        if let Some(ref output_path) = cli.output {
            let json_output = JsonOutput::new();
            json_output.write_to_file(result, output_path).await?;
            println!("📄 Results saved to: {}", output_path.display());
        }

        Ok(())
    }
    
    /// Print circular dependencies in a user-friendly format
    fn print_circular_dependencies(circulars: &[Vec<String>], take_limit: Option<usize>) {
        if circulars.is_empty() {
            println!("{}", style("🔄 Circular Dependencies").bold().cyan());
            println!("  {} {}", 
                style("✅").green(),
                style("Congratulations, no circular dependency was found in your project.").green()
            );
        } else {
            println!("{}", style("⚠️  Circular Dependencies").bold().yellow());
            for (i, circular) in circulars.iter().enumerate() {
                println!("  {}) {}", 
                    style(i + 1).bold(),
                    circular.join(" → ")
                );
            }
            
            if let Some(limit) = take_limit {
                if circulars.len() >= limit {
                    println!("  {} {} (search limit reached)",
                        style("At least").dim(),
                        style(format!("{} circular dependencies found", limit)).bold()
                    );
                }
            }
        }
        println!();
    }
    
    /// Count total dependencies in the dependency tree
    fn count_total_dependencies(tree: &crate::types::DependencyTree) -> usize {
        tree.values()
            .filter_map(|deps| deps.as_ref())
            .map(|deps| deps.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DependencyTree;

    #[test]
    fn test_count_total_dependencies() {
        let mut tree = DependencyTree::new();
        tree.insert("file1.js".to_string(), Some(vec![]));
        tree.insert("file2.js".to_string(), None);
        
        assert_eq!(AnalysisRunner::count_total_dependencies(&tree), 0);
    }
    
    #[test]
    fn test_print_circular_dependencies_empty() {
        // This test mainly ensures the function doesn't panic
        AnalysisRunner::print_circular_dependencies(&[], None);
    }
}
