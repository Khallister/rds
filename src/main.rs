mod analyzer;
mod cache;
mod config;
mod output;
mod parser;
mod types;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use console::style;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;

use crate::analyzer::DependencyAnalyzer;
use crate::types::{ParseOptions, SkipDynamicImports};
use crate::output::{ConsoleOutput, JsonOutput};

async fn expand_file_inputs(inputs: &[String], filter: &Option<String>) -> Result<Vec<String>> {
    let mut expanded_files = Vec::new();
    
    // Parse filter extensions if provided
    let filter_extensions: Option<Vec<String>> = filter.as_ref().map(|f| {
        f.split(',')
            .map(|ext| {
                let ext = ext.trim();
                if ext.starts_with('.') {
                    ext.to_string()
                } else {
                    format!(".{}", ext)
                }
            })
            .collect()
    });
    
    for input in inputs {
        let path = Path::new(input);
        
        if path.is_dir() {
            // Scan directory for supported files
            let dir_files = scan_directory(path, &filter_extensions).await?;
            expanded_files.extend(dir_files);
        } else if path.is_file() {
            // Check if file matches filter (if provided)
            if should_include_file(path, &filter_extensions) {
                expanded_files.push(input.clone());
            }
        } else {
            // Treat as glob pattern
            expanded_files.push(input.clone());
        }
    }
    
    // Remove duplicates and sort
    expanded_files.sort();
    expanded_files.dedup();
    
    Ok(expanded_files)
}

fn scan_directory<'a>(
    dir: &'a Path,
    filter_extensions: &'a Option<Vec<String>>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>>> + Send + 'a>> {
    Box::pin(async move {
        let mut files = Vec::new();
        let mut read_dir = tokio::fs::read_dir(dir).await?;
        
        // Create a simple exclusion regex for directory names
        let exclusion_regex = regex::Regex::new(r"node_modules|\.git|\.svn|\.hg|coverage|dist|build|out|\.next|\.nuxt")?;
        
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            
            if path.is_dir() {
                // Check if directory should be excluded
                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                    if !exclusion_regex.is_match(dir_name) {
                        // Recursively scan non-excluded subdirectories
                        let sub_files = Box::pin(scan_directory(&path, filter_extensions)).await?;
                        files.extend(sub_files);
                    }
                }
            } else if path.is_file() && should_include_file(&path, filter_extensions) {
                files.push(path.to_string_lossy().to_string());
            }
        }
        
        Ok(files)
    })
}

fn should_include_file(path: &Path, filter_extensions: &Option<Vec<String>>) -> bool {
    let extension = path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext))
        .unwrap_or_default();
    
    match filter_extensions {
        Some(filters) => {
            filters.iter().any(|filter_ext| {
                extension == *filter_ext
            })
        }
        None => {
            // Default supported extensions
            matches!(extension.as_str(), 
                ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".json" | ".vue"
            )
        }
    }
}

#[derive(Parser, Clone)]
#[command(name = "rds")]
#[command(about = "A memory-efficient dependency analyzer for JavaScript, TypeScript, and Vue projects")]
#[command(version)]
pub struct Cli {
    #[arg(required = true, help = "Input files or directories to analyze")]
    files: Vec<String>,
    
    #[arg(long, help = "Base directory for resolving relative paths")]
    context: Option<PathBuf>,
    
    #[arg(long, alias = "ext", default_value = ".ts,.tsx,.mjs,.js,.jsx,.json,.vue", 
          help = "File extensions to analyze (comma-separated)")]
    extensions: String,
    
    #[arg(long, default_value = ".ts,.tsx,.mjs,.js,.jsx", 
          help = "JavaScript file extensions (comma-separated)")]
    js: String,
    
    #[arg(long, help = "Filter files by extension when scanning directories (e.g., 'js,ts,vue')")]
    filter: Option<String>,
    
    #[arg(long, default_value = ".*", help = "Regex pattern for files to include")]
    include: String,
    
    #[arg(long, default_value = "node_modules|\\.git|\\.svn|\\.hg|coverage|dist|build|out|\\.next|\\.nuxt",
          help = "Regex pattern for files/directories to exclude")]
    exclude: String,
    
    #[arg(short = 'o', long, help = "Output file path for JSON results")]
    output: Option<PathBuf>,
    
    #[arg(long, action = clap::ArgAction::SetTrue, help = "Show dependency tree visualization")]
    tree: bool,
    
    #[arg(long, action = clap::ArgAction::SetTrue, 
          help = "Detect and show circular dependencies")]
    circular: bool,
    
    #[arg(long, action = clap::ArgAction::SetTrue, help = "Show warning messages during analysis")]
    warning: bool,
    
    #[arg(long, action = clap::ArgAction::SetTrue, help = "Enable verbose logging output")]
    log: bool,
    
    #[arg(long, action = clap::ArgAction::SetTrue, help = "Exit with code 1 if circular dependencies are found")]
    throw: bool,
    
    #[arg(long, help = "Path to tsconfig.json for TypeScript path resolution")]
    tsconfig: Option<PathBuf>,
    
    #[arg(short = 'T', long, help = "Enable code transformations during parsing")]
    transform: bool,
    
    #[arg(long, help = "Custom exit codes (format: 'case:code,case:code')")]
    exit_code: Option<String>,
    
    #[arg(long, help = "Show progress bar (auto-detected if not specified)")]
    progress: Option<bool>,
    
    #[arg(long, help = "Pattern to detect unused files from")]
    detect_unused_files_from: Option<String>,
    
    #[arg(long, value_enum, help = "Skip dynamic imports in tree or circular analysis")]
    skip_dynamic_imports: Option<SkipDynamicImportsArg>,
    
    #[arg(long, help = "Maximum number of circular dependencies to find before stopping")]
    take: Option<usize>,
    
    #[arg(short = 'W', long, action = clap::ArgAction::SetTrue, 
          help = "Watch mode: monitor files for changes and re-run analysis")]
    watch: bool,
    
    #[arg(long, action = clap::ArgAction::SetTrue, 
          help = "Enable file caching to speed up repeated analysis")]
    cache: bool,
    
    #[arg(long, action = clap::ArgAction::SetTrue, 
          help = "Disable file caching (override default)")]
    no_cache: bool,
}

#[derive(Clone, ValueEnum)]
pub enum SkipDynamicImportsArg {
    Tree,
    Circular,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    if cli.watch {
        run_watch_mode(&cli).await
    } else {
        run_analysis_once(&cli).await
    }
}

async fn run_watch_mode(cli: &Cli) -> Result<()> {
    println!("{} {} ({})", 
        style("👁️").bright().bold(),
        style("Starting watch mode...").bold().cyan(),
        style(format!("rds v{}", env!("CARGO_PKG_VERSION"))).dim()
    );
    
    // Expand directories and apply filters
    let expanded_files = expand_file_inputs(&cli.files, &cli.filter).await?;
    if expanded_files.is_empty() {
        eprintln!("No files found to watch.");
        return Ok(());
    }
    
    // Set up file watcher
    let (tx, mut rx) = mpsc::channel(100);
    
    // Create a watcher
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    if let Err(e) = tx.blocking_send(event) {
                        eprintln!("Watch channel send error: {}", e);
                    }
                },
                Err(e) => eprintln!("Watch error: {}", e),
            }
        },
        Config::default(),
    )?;
    
    // Watch all directories that contain our target files
    let mut watched_dirs = std::collections::HashSet::new();
    for file in &expanded_files {
        if let Some(parent) = Path::new(file).parent() {
            watched_dirs.insert(parent.to_path_buf());
        }
    }
    
    // Also watch the input directories directly
    for input in &cli.files {
        let path = Path::new(input);
        if path.is_dir() {
            watched_dirs.insert(path.to_path_buf());
        } else if let Some(parent) = path.parent() {
            watched_dirs.insert(parent.to_path_buf());
        }
    }
    
    for dir in &watched_dirs {
        if dir.exists() {
            watcher.watch(dir, RecursiveMode::Recursive)?;
            println!("{} Watching: {}", 
                style("📂").cyan(),
                style(dir.display()).dim()
            );
        }
    }
    
    println!("{}", 
        style("💡 Press Ctrl+C to exit. Files will be analyzed incrementally on change.").dim()
    );
    
    // Track changed files for intelligent analysis with cancellation
    let mut changed_files: HashSet<String> = HashSet::new();
    let mut last_change = Instant::now();
    let mut analysis_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut logged_files: HashSet<String> = HashSet::new(); // Track which files we've already logged
    
    // Create persistent analyzer for watch mode to maintain cache between analyses
    let mut watch_options = ParseOptions::default();
    if let Some(context) = &cli.context {
        watch_options.context = context.clone();
    }
    watch_options.extensions = cli.extensions.split(',').map(|s| s.to_string()).collect();
    watch_options.js_extensions = cli.js.split(',').map(|s| s.to_string()).collect();
    watch_options.include = regex::Regex::new(&cli.include)?;
    watch_options.exclude = regex::Regex::new(&cli.exclude)?;
    watch_options.dependency_exclude = regex::Regex::new(r"node_modules|\.git|\.svn|\.hg")?;
    watch_options.tsconfig = cli.tsconfig.clone();
    watch_options.transform = cli.transform;
    watch_options.take = cli.take;
    watch_options.skip_dynamic_imports = match cli.skip_dynamic_imports {
        Some(SkipDynamicImportsArg::Tree) => SkipDynamicImports::Tree,
        Some(SkipDynamicImportsArg::Circular) => SkipDynamicImports::Circular,
        None => SkipDynamicImports::Never,
    };
    // Enable caching for watch mode
    watch_options.cache_enabled = !cli.no_cache;
    // Disable progress callback for watch mode
    watch_options.progress_callback = None;
    
    let persistent_analyzer = Arc::new(tokio::sync::Mutex::new(DependencyAnalyzer::new(watch_options)?));
    
    // Handle file system events
    const DEBOUNCE_DURATION: std::time::Duration = std::time::Duration::from_millis(300);
    
    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                let new_changed_files = extract_relevant_file_changes(&event, &expanded_files);
                if !new_changed_files.is_empty() {
                    let mut any_new_files = false;
                    
                    for file in new_changed_files {
                        if changed_files.insert(file.clone()) {
                            // This is a new file in this debounce cycle
                            if !logged_files.contains(&file) {
                                println!("{} File change detected: {}", 
                                    style("📝").dim(),
                                    style(&file).dim()
                                );
                                logged_files.insert(file);
                                any_new_files = true;
                            }
                        }
                    }
                    
                    // Update last change time if we had new files
                    if any_new_files {
                        last_change = Instant::now();
                    }
                    
                    // Cancel any running analysis if we have any changes
                    if !changed_files.is_empty() {
                        if let Some(task) = analysis_task.take() {
                            task.abort();
                            println!("{} Cancelling previous analysis, {} changes queued", 
                                style("🔄").yellow(),
                                style(changed_files.len()).bold()
                            );
                        }
                    }
                }
            },
            _ = tokio::time::sleep(DEBOUNCE_DURATION) => {
                if !changed_files.is_empty() && analysis_task.is_none() && last_change.elapsed() >= DEBOUNCE_DURATION {
                    let files_to_analyze: Vec<String> = changed_files.drain().collect();
                    logged_files.clear(); // Clear logged files for next cycle
                    
                    println!("\n{} Analyzing {} changed file(s) and their dependencies...", 
                        style("🔍").bright(),
                        style(files_to_analyze.len()).bold().yellow()
                    );
                    
                    // Show which files changed
                    for file in &files_to_analyze {
                        println!("  {} {}", 
                            style("�").dim(),
                            style(file).dim()
                        );
                    }
                    
                    // Spawn cancellable analysis task
                    let analyzer_clone = persistent_analyzer.clone();
                    let files_to_analyze_clone = files_to_analyze.clone();
                    
                    analysis_task = Some(tokio::spawn(async move {
                        let start_time = Instant::now();
                        let file_count = files_to_analyze_clone.len();
                        
                        // Use the persistent analyzer
                        let mut analyzer = analyzer_clone.lock().await;
                        match analyzer.analyze_files_incremental(&files_to_analyze_clone).await {
                            Ok((result, num_threads)) => {
                                let elapsed = start_time.elapsed();
                                let cache_stats = analyzer.get_incremental_cache_stats(); // Get incremental stats
                                
                                // Compact output for watch mode with cache info
                                let cache_info = if cache_stats.hits > 0 {
                                    format!(", {}💾", cache_stats.hits)
                                } else {
                                    String::new()
                                };
                                
                                println!("  {} {} files, {} deps ({:.2}s, {} threads{})", 
                                    style("📊").bright(),
                                    style(file_count).yellow(),
                                    style(result.tree.len()).yellow(),
                                    elapsed.as_secs_f64(),
                                    style(num_threads).cyan(),
                                    cache_info
                                );
                                
                                // Show results based on CLI flags
                                if result.circulars.len() > 0 {
                                    let console_output = ConsoleOutput::new();
                                    console_output.print_circular(&result.circulars, None);
                                }
                            },
                            Err(e) => {
                                eprintln!("{} Analysis failed: {}\n", 
                                    style("❌").bright().red(),
                                    e
                                );
                            }
                        }
                    }));
                }
            },
            // Check if analysis task completed
            Some(_) = async {
                match &mut analysis_task {
                    Some(task) => task.await.ok(),
                    None => std::future::pending().await,
                }
            } => {
                analysis_task = None;
            },
            _ = tokio::signal::ctrl_c() => {
                println!("\n{} {}", 
                    style("👋").bright(),
                    style("Stopping watch mode...").bold().cyan()
                );
                break;
            }
        }
    }
    
    Ok(())
}

fn extract_relevant_file_changes(event: &Event, _watched_files: &[String]) -> Vec<String> {
    let mut changed_files = Vec::new();
    
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            for path in &event.paths {
                let path_str = path.to_string_lossy();
                
                // Check if this file matches our watched extensions
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if matches!(ext, "js" | "jsx" | "ts" | "tsx" | "vue" | "mjs" | "json") {
                        // Skip node_modules and other irrelevant directories
                        if !path_str.contains("node_modules") 
                           && !path_str.contains(".git") 
                           && !path_str.contains("dist")
                           && !path_str.contains("build") {
                            
                            // Normalize the path and add it
                            let normalized = normalize_path_string(&path_str);
                            changed_files.push(normalized);
                        }
                    }
                }
            }
        },
        _ => {}
    }
    
    changed_files
}

fn normalize_path_string(path: &str) -> String {
    Path::new(path).to_string_lossy().replace("\\", "/")
}

async fn run_analysis_once(cli: &Cli) -> Result<()> {
    let show_progress = cli.progress.unwrap_or_else(|| {
        atty::is(atty::Stream::Stdout) && std::env::var("CI").is_err()
    });

    // Expand directories and apply filters
    let expanded_files = expand_file_inputs(&cli.files, &cli.filter).await?;
    if expanded_files.is_empty() {
        eprintln!("No files found to analyze.");
        return Ok(());
    }

    let mut options = ParseOptions::default();
    
    if let Some(context) = &cli.context {
        options.context = context.clone();
    }
    
    options.extensions = cli.extensions.split(',').map(|s| s.to_string()).collect();
    options.js_extensions = cli.js.split(',').map(|s| s.to_string()).collect();
    options.include = regex::Regex::new(&cli.include)?;
    options.exclude = regex::Regex::new(&cli.exclude)?;
    
    // For dependency analysis, exclude external packages and VCS directories
    // This resolves but doesn't recursively analyze node_modules dependencies
    options.dependency_exclude = regex::Regex::new(r"node_modules|\.git|\.svn|\.hg")?;
    
    options.tsconfig = cli.tsconfig.clone();
    options.transform = cli.transform;
    options.take = cli.take;
    
    options.skip_dynamic_imports = match cli.skip_dynamic_imports {
        Some(SkipDynamicImportsArg::Tree) => SkipDynamicImports::Tree,
        Some(SkipDynamicImportsArg::Circular) => SkipDynamicImports::Circular,
        None => SkipDynamicImports::Never,
    };
    
    // Configure caching based on CLI flags
    options.cache_enabled = if cli.no_cache {
        false
    } else if cli.cache {
        true
    } else {
        // Default behavior: enable cache for watch mode, disable for single runs
        cli.watch
    };
    
    // Set up progress callback with enhanced UI
    let file_count = expanded_files.len();
    let start_time = Instant::now();
    let multi_progress = MultiProgress::new();
    let main_pb = multi_progress.add(ProgressBar::new(0)); // Start with 0, will be updated dynamically
    
    main_pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-")
    );
    
    if show_progress {
        let total = Arc::new(Mutex::new(0));
        let current = Arc::new(Mutex::new(0));
        let total_clone = total.clone();
        let current_clone = current.clone();
        let log_enabled = cli.log;
        let pb_clone = main_pb.clone();
        
        options.progress_callback = Some(Box::new(move |event, target| {
            match event {
                crate::types::ProgressEvent::Start => {
                    let new_total = {
                        let mut total = total_clone.lock().unwrap();
                        *total += 1;
                        *total
                    };
                    pb_clone.set_length(new_total as u64);
                    if log_enabled {
                        pb_clone.println(format!("📁 Analyzing {}", style(target).cyan()));
                    }
                }
                crate::types::ProgressEvent::End => {
                    let current_count = {
                        let mut current = current_clone.lock().unwrap();
                        *current += 1;
                        *current
                    };
                    pb_clone.set_position(current_count as u64);
                    pb_clone.set_message(format!("Processing files..."));
                }
            }
        }));
    }
    
    // Start analysis with enhanced output
    if !cli.watch {
        println!("{} {} ({})", 
            style("🚀").bright().bold(),
            style("Starting dependency analysis...").bold().blue(),
            style(format!("rds v{}", env!("CARGO_PKG_VERSION"))).dim()
        );
    }
    
    if show_progress {
        main_pb.set_message("Initializing...".to_string());
    }
    
    let mut analyzer = DependencyAnalyzer::new(options)?;
    let (result, num_threads) = analyzer.analyze_files(&expanded_files).await?;
    
    if show_progress {
        main_pb.finish_with_message("Complete!".to_string());
    }
    
    let elapsed = start_time.elapsed();
    
    // Get cache statistics
    let cache_stats = analyzer.get_cache_stats();
    
    // Enhanced completion message with statistics
    if !cli.watch {
        println!("{} {} {}", 
            style("✨").bright().bold(),
            style("Analysis complete!").bold().green(),
            style(format!("({:.2}s)", elapsed.as_secs_f64())).dim()
        );
        
        println!("{} {} files processed, {} total dependencies in tree", 
            style("📊").bright(),
            style(file_count).bold().yellow(),
            style(result.tree.len()).bold().yellow()
        );
        
        println!("{} Analysis used {} threads for parallel processing", 
            style("🧵").bright(),
            style(num_threads).bold().cyan()
        );
        
        // Show cache statistics if caching was enabled
        if cache_stats.cached_files > 0 || cache_stats.hits > 0 || cache_stats.misses > 0 {
            println!("{} Cache: {} hits, {} misses ({:.1}% hit rate), {} files cached", 
                style("💾").bright(),
                style(cache_stats.hits).bold().green(),
                style(cache_stats.misses).bold().yellow(),
                cache_stats.hit_rate,
                style(cache_stats.cached_files).bold().blue()
            );
        }
    } else {
        // Compact output for watch mode
        let cache_info = if cache_stats.hits > 0 {
            format!(", {}💾", cache_stats.hits)
        } else {
            String::new()
        };
        
        println!("  {} {} files, {} deps ({:.2}s, {} threads{})", 
            style("📊").bright(),
            style(file_count).yellow(),
            style(result.tree.len()).yellow(),
            elapsed.as_secs_f64(),
            style(num_threads).cyan(),
            cache_info
        );
    }
    
    // Output JSON if requested
    if let Some(output_path) = &cli.output {
        let json_output = JsonOutput::new();
        json_output.write_to_file(&result, output_path).await?;
    }
    
    // Console output
    let console_output = ConsoleOutput::new();
    
    // Default behavior: show both tree and circular if neither flag is explicitly set
    let show_tree = cli.tree || (!cli.tree && !cli.circular);
    let show_circular = cli.circular || (!cli.tree && !cli.circular);
    
    if show_tree {
        console_output.print_tree(&result.tree, &result.entries);
    }
    
    if show_circular {
        console_output.print_circular(&result.circulars, cli.take);
    }
    
    if cli.warning {
        let warnings = analyzer.analyze_warnings(&result.tree);
        console_output.print_warnings(&warnings);
    }
    
    // Handle unused files detection
    if let Some(pattern) = &cli.detect_unused_files_from {
        let unused = analyzer.detect_unused_files(pattern, &result.tree).await?;
        console_output.print_unused_files(&unused);
    }
    
    // Handle --throw flag for CI/CD pipelines
    if cli.throw && !result.circulars.is_empty() {
        eprintln!("{} {} circular dependencies found. Exiting with code 1.", 
            style("❌").bright().bold(),
            style("Error:").bold().red()
        );
        std::process::exit(1);
    }
    
    // Handle exit codes
    if let Some(exit_code_spec) = &cli.exit_code {
        handle_exit_codes(exit_code_spec, &result.circulars)?;
    }
    
    Ok(())
}

fn handle_exit_codes(spec: &str, circulars: &[Vec<String>]) -> Result<()> {
    for part in spec.split(',') {
        let parts: Vec<&str> = part.split(':').collect();
        if parts.len() != 2 {
            anyhow::bail!("Invalid exit code format: {}", part);
        }
        
        let case = parts[0];
        let code: i32 = parts[1].parse()?;
        
        match case {
            "circular" if !circulars.is_empty() => {
                std::process::exit(code);
            }
            "circular" => {}
            _ => anyhow::bail!("Unsupported exit case: {}", case),
        }
    }
    
    Ok(())
}
