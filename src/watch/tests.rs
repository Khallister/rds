use super::*;
use crate::analyzer::DependencyAnalyzer;
use crate::cli::Cli;
use crate::types::DependencyTree;
use crate::types::ParseOptions;

#[tokio::test]
async fn test_run_incremental_analysis_print_circular_only() -> anyhow::Result<()> {
    let analyzer = DependencyAnalyzer::new(ParseOptions::default())?;
    let analyzer = std::sync::Arc::new(tokio::sync::Mutex::new(analyzer));

    let cli = Cli {
        files: vec![".".to_string()],
        context: None,
        extensions: ".js".to_string(),

        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules|\\.git".to_string(),
        output: None,
        tree: false,
        circular: true,
        warning: false,
        log: false,
        throw: false,
        tsconfig: None,

        exit_code: None,
        progress: None,
        detect_unused_files_from: None,
        skip_dynamic_imports: None,
        take: None,
        watch: false,
        cache: false,
        no_cache: false,
        threads: None,
    };

    // changed_files empty should exercise analyze_files_incremental empty path
    WatchRunner::run_incremental_analysis(analyzer, Vec::new(), &cli).await?;
    Ok(())
}

#[tokio::test]
async fn test_run_incremental_analysis_print_tree_only() -> anyhow::Result<()> {
    let analyzer = DependencyAnalyzer::new(ParseOptions::default())?;
    let analyzer = std::sync::Arc::new(tokio::sync::Mutex::new(analyzer));

    let cli = Cli {
        files: vec![".".to_string()],
        context: None,
        extensions: ".js".to_string(),

        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules|\\.git".to_string(),
        output: None,
        tree: true,
        circular: false,
        warning: false,
        log: false,
        throw: false,
        tsconfig: None,

        exit_code: None,
        progress: None,
        detect_unused_files_from: None,
        skip_dynamic_imports: None,
        take: None,
        watch: false,
        cache: false,
        no_cache: false,
        threads: None,
    };

    WatchRunner::run_incremental_analysis(analyzer, Vec::new(), &cli).await?;
    Ok(())
}

#[test]
fn test_count_total_dependencies() {
    let mut tree = DependencyTree::new();
    tree.insert("file1.js".to_string(), Some(vec![]));
    tree.insert("file2.js".to_string(), None);

    assert_eq!(WatchRunner::count_total_dependencies(&tree), 0);
}

#[test]
fn test_print_circular_dependencies_compact() {
    let out = ConsoleOutput::new();
    out.print_circular(&[], None, Some(3));
}
