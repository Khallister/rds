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
use crate::logger;
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

        logger::debug(&format!("Watch mode expanding inputs: {:?}", &cli.files));
        let expanded_files = FileSystem::expand_file_inputs(&cli.files, &cli.filter).await?;
        logger::info(&format!(
            "Watch mode will monitor {} files",
            expanded_files.len()
        ));
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

        // Build parse options for watch mode
        let watch_options = config::create_parse_options_from_cli(cli)?;
        // Keep a clone of options for event filtering so we don't need to lock
        // the persistent analyzer just to access exclude/include patterns.
        let event_filter_options = watch_options.clone();

        let persistent_analyzer = Arc::new(tokio::sync::Mutex::new(DependencyAnalyzer::new(
            watch_options,
        )?));

        // If pre_scan is requested, run a full analysis once before entering watch loop.
        // Run the pre-scan on the persistent analyzer so its internal state
        // (last_full_tree, reverse_index) is populated and incremental runs
        // can use that data to compute affected sets.
        if cli.pre_scan {
            println!(
                "{}",
                style("🔎 Running initial full scan before watch...").blue()
            );
            let start_time = Instant::now();
            let mut analyzer_guard = persistent_analyzer.lock().await;
            let (result, num_threads) = analyzer_guard.analyze_files(&expanded_files).await?;
            let duration = start_time.elapsed();

            println!(
                "  📊 {} files, {} deps ({:.2?}, {} threads)",
                expanded_files.len(),
                Self::count_total_dependencies(&result.tree),
                duration,
                num_threads
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

            println!("{}", style("✅ Initial scan complete.").green());
        }

        const DEBOUNCE_DURATION: std::time::Duration = std::time::Duration::from_millis(300);

        let mut first_incremental_run = true;

        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    let relevant_changes = extract_relevant_file_changes(
                        &event,
                        &expanded_files,
                        &event_filter_options.exclude,
                    );

                    if !relevant_changes.is_empty() {
                        logger::debug(&format!("Relevant changes: {:?}", &relevant_changes));
                        Self::apply_relevant_changes(
                            &mut changed_files,
                            &mut logged_files,
                            &mut analysis_task,
                            &mut last_change,
                            relevant_changes,
                        );
                    }
                }

                _ = tokio::time::sleep(DEBOUNCE_DURATION) => {
                    logger::debug("Debounce tick");
                    if !changed_files.is_empty() && last_change.elapsed() >= DEBOUNCE_DURATION {
                        let files_to_analyze: Vec<String> = changed_files.drain().collect();
                        logged_files.clear();

                        let analyzer = Arc::clone(&persistent_analyzer);
                        let cli_clone = cli.clone();

                        logger::info(&format!("Triggering incremental analysis for {} files", files_to_analyze.len()));
                        // For the very first incremental trigger (when the persistent
                        // analyzer has not yet been populated via pre-scan), prefer
                        // running a full scan once so the analyzer's last_full_tree
                        // and reverse_index are initialized. Afterwards prefer the
                        // incremental path by passing an empty all_files vector.
                        let all_files_clone: Vec<String> = if first_incremental_run {
                            first_incremental_run = false;
                            expanded_files.clone()
                        } else {
                            Vec::new()
                        };
                        analysis_task = Some(tokio::spawn(async move {
                            if let Err(e) = Self::run_incremental_analysis(
                                analyzer,
                                files_to_analyze,
                                all_files_clone,
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
        all_files: Vec<String>,
        cli: &Cli,
    ) -> Result<()> {
        println!("{}", style("🔄 Running analysis...").blue());
        let start_time = Instant::now();

        let mut analyzer = analyzer.lock().await;

        // Invalidate caches targeted to the changed files before analysis begins.
        // If the number of changed files is large, prefer clearing all caches.
        let invalidate_threshold = 100usize;
        if changed_files.len() > invalidate_threshold {
            if std::env::var("RDS_WATCH_DEBUG").is_ok() {
                crate::logger::info(&format!(
                    "[Watch] large change set ({} files), clearing all caches",
                    changed_files.len()
                ));
            }
            analyzer.clear_all_caches().await;
        } else {
            analyzer.invalidate_caches(&changed_files).await;
        }

        // Prefer a full analysis run when the caller passes the complete list
        // of files to analyze. This ensures cycles that involve files outside
        // the changed_files set are detected.
        let used_full = !all_files.is_empty();
        let (result, num_threads) = if used_full {
            analyzer.analyze_files(&all_files).await?
        } else {
            analyzer.analyze_files_incremental(&changed_files).await?
        };

        if std::env::var("RDS_WATCH_DEBUG").is_ok() {
            let circulars_count = result.circulars.len();
            let circulars_sample: Vec<Vec<String>> =
                result.circulars.iter().take(5).cloned().collect();
            let tree_keys_sample: Vec<String> = result.tree.keys().cloned().take(10).collect();

            crate::logger::info(&format!(
                "[Watch] used_full={}, entries={:?}, circulars_count={}, circulars_sample={:?}, tree_keys_count={}, tree_keys_sample={:?}",
                used_full,
                result.entries,
                circulars_count,
                circulars_sample,
                result.tree.len(),
                tree_keys_sample
            ));
        }

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

    fn apply_relevant_changes(
        changed_files: &mut HashSet<String>,
        logged_files: &mut HashSet<String>,
        analysis_task: &mut Option<tokio::task::JoinHandle<()>>,
        last_change: &mut Instant,
        relevant_changes: Vec<String>,
    ) {
        for file in relevant_changes {
            changed_files.insert(file.clone());

            if !logged_files.contains(&file) {
                println!("📝 File change detected: {}", style(&file).yellow());
                logged_files.insert(file);
            }
        }

        *last_change = Instant::now();

        if let Some(task) = analysis_task.take() {
            task.abort();
        }
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
