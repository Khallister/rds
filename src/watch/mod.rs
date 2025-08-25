//! Watch mode functionality for monitoring file changes and re-running analysis.

use anyhow::Result;
use console::style;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

use crate::analyzer::DependencyAnalyzer;
use crate::cli::Cli;
use crate::filesystem::FileSystem;
use crate::output::ConsoleOutput;
use crate::utils::{config, extract_relevant_file_changes};

pub struct WatchRunner;

impl WatchRunner {
    pub async fn run_watch_mode(cli: &Cli) -> Result<()> {
        println!(
            "{} {} ({} )",
            style("👁️").blue(),
            style("Starting watch mode...").bold().blue(),
            style("Press Ctrl+C to exit").dim()
        );

        let expanded_files = FileSystem::expand_file_inputs(&cli.files, &cli.filter).await?;
        if expanded_files.is_empty() {
            eprintln!("No files found matching the specified criteria");
            return Ok(());
        }

        let (tx, mut rx) = mpsc::channel(100);

        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if let Err(e) = tx.try_send(event) {
                        eprintln!("Failed to send file event: {}", e);
                    }
                }
            },
            Config::default(),
        )?;

        let watch_dirs = FileSystem::get_watch_directories(&expanded_files);

        for input in &cli.files {
            let path = Path::new(input);
            if path.is_dir() {
                if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
                    eprintln!("Warning: Failed to watch directory {}: {}", input, e);
                }
            }
        }

        for dir in &watch_dirs {
            let path = Path::new(dir);
            if path.exists() && path.is_dir() {
                if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
                    eprintln!("Warning: Failed to watch directory {}: {}", dir, e);
                }
            }
        }

        println!("{}", style("📂 Watching directories for changes...").dim());
        for dir in &watch_dirs {
            println!("📂 Watching: {}", style(dir).cyan());
        }
        println!(
            "{}",
            style("💡 Press Ctrl+C to exit, or modify files to trigger analysis").dim()
        );

        let mut changed_files: HashSet<String> = HashSet::new();
        let mut last_change = Instant::now();
        let mut analysis_task: Option<tokio::task::JoinHandle<()>> = None;
        let mut logged_files: HashSet<String> = HashSet::new();

        let watch_options = config::create_parse_options_from_cli(cli)?;
        let persistent_analyzer = Arc::new(tokio::sync::Mutex::new(DependencyAnalyzer::new(
            watch_options,
        )?));

        const DEBOUNCE_DURATION: std::time::Duration = std::time::Duration::from_millis(300);

        loop {
            tokio::select! {
                              Some(event) = rx.recv() => {
                    let relevant_changes = extract_relevant_file_changes(&event, &expanded_files, &persistent_analyzer.lock().await.options().exclude);

                    if !relevant_changes.is_empty() {
                        for file in relevant_changes {
                            changed_files.insert(file.clone());

                                                      if !logged_files.contains(&file) {
                                println!("📝 File change detected: {}", style(&file).yellow());
                                logged_files.insert(file);
                            }
                        }
                        last_change = Instant::now();

                                              if let Some(task) = analysis_task.take() {
                            task.abort();
                        }
                    }
                }

                               _ = tokio::time::sleep(DEBOUNCE_DURATION) => {
                    if !changed_files.is_empty() &&
                       last_change.elapsed() >= DEBOUNCE_DURATION {

                        let files_to_analyze: Vec<String> = changed_files.drain().collect();
                        logged_files.clear();

                                              let analyzer = Arc::clone(&persistent_analyzer);
                        let cli_clone = cli.clone();

                                              analysis_task = Some(tokio::spawn(async move {
                            if let Err(e) = Self::run_incremental_analysis(
                                analyzer,
                                files_to_analyze,
                                &cli_clone
                            ).await {
                                eprintln!("Analysis error: {}", e);
                            }
                        }));
                    }
                }

                              _ = tokio::signal::ctrl_c() => {
                    println!("\n{}", style("🛑 Stopping watch mode...").yellow());
                    break;
                }
            }
        }

        Ok(())
    }

    async fn run_incremental_analysis(
        analyzer: Arc<tokio::sync::Mutex<DependencyAnalyzer>>,
        changed_files: Vec<String>,
        cli: &Cli,
    ) -> Result<()> {
        println!("{}", style("🔄 Running analysis...").blue());
        let start_time = Instant::now();

        let mut analyzer = analyzer.lock().await;
        let (result, num_threads) = analyzer.analyze_files_incremental(&changed_files).await?;

        let cache_stats = analyzer.get_incremental_cache_stats();

        let duration = start_time.elapsed();

        print!("\x1b[2J\x1b[H");
        println!(
            "  📊 {} files, {} deps ({:.2?}, {} threads)",
            changed_files.len(),
            Self::count_total_dependencies(&result.tree),
            duration,
            num_threads
        );

        println!(
            "  🗄️  Cache: {} hits, {} misses, {} files cached, {} tree reuses (hit rate {:.1}%)",
            cache_stats.hits,
            cache_stats.misses,
            cache_stats.cached_files,
            cache_stats.cached_tree_reuses,
            cache_stats.hit_rate
        );

        let show_circular = cli.circular || (!cli.circular && !cli.tree);

        if show_circular {
            let console_output = ConsoleOutput::new();
            console_output.print_circular(&result.circulars, cli.take, Some(3));
        }

        if cli.tree {
            let console_output = ConsoleOutput::new();
            console_output.print_tree(&result.tree, &result.entries);
        }

        println!(
            "{}",
            style("✅ Analysis complete, watching for changes...").green()
        );

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
