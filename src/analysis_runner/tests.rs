use super::*;
use crate::types::DependencyTree;
use crate::types::{AnalysisResult, Dependency, DependencyKind};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn test_count_total_dependencies() {
    let mut tree = DependencyTree::new();
    tree.insert("file1.js".to_string(), Some(vec![]));
    tree.insert("file2.js".to_string(), None);

    assert_eq!(AnalysisRunner::count_total_dependencies(&tree), 0);
}

#[test]
fn test_print_circular_dependencies_empty() {
    let out = ConsoleOutput::new();
    let res = std::panic::catch_unwind(|| {
        out.print_circular(&[], None, None);
    });
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_display_analysis_results_writes_json_and_prints() -> anyhow::Result<()> {
    // prepare a simple dependency tree with one dependency
    let mut tree: DependencyTree = DependencyTree::new();
    let dep = Dependency {
        issuer: "a.js".to_string(),
        request: "b.js".to_string(),
        kind: DependencyKind::StaticImport,
        id: None,
    };
    tree.insert("a.js".to_string(), Some(vec![dep]));

    let entries = vec!["a.js".to_string()];
    let result = AnalysisResult {
        entries: entries.clone(),
        tree: tree.clone(),
        circulars: Vec::new(),
    };

    // create a temp file for JSON output
    let td = tempdir()?;
    let out_path = td.path().join("out.json");

    // build a CLI that requests tree output and JSON file
    let cli = Cli {
        files: vec![],
        context: None,
        extensions: ".js".to_string(),
        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules|\\.git".to_string(),
        output: Some(PathBuf::from(&out_path)),
        tree: true,
        circular: false,
        log: false,
        throw: false,
        tsconfig: None,

        exit_code: None,
        progress: false,
        skip_dynamic_imports: false,
        take: None,
        watch: false,
        cache: false,
        no_cache: false,
        threads: None,
    };

    // call the private display function directly
    AnalysisRunner::display_analysis_results(&result, &entries, Duration::from_millis(10), 1, &cli)
        .await?;

    // ensure JSON file exists and contains the expected structure
    let s = std::fs::read_to_string(&out_path)?;
    let parsed: AnalysisResult = serde_json::from_str(&s)?;
    assert_eq!(parsed.entries, result.entries);

    Ok(())
}

#[tokio::test]
async fn test_run_analysis_once_with_empty_inputs_returns_ok() -> anyhow::Result<()> {
    let cli = Cli {
        files: vec![],
        context: None,
        extensions: ".js".to_string(),

        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules|\\.git".to_string(),
        output: None,
        tree: false,
        circular: false,
        log: false,
        throw: false,
        tsconfig: None,

        exit_code: None,
        progress: false,
        skip_dynamic_imports: false,
        take: None,
        watch: false,
        cache: false,
        no_cache: false,
        threads: None,
    };

    let res = AnalysisRunner::run_analysis_once(&cli).await?;
    let _ = res;
    Ok(())
}

#[tokio::test]
async fn test_run_analysis_once_with_exit_code_and_no_circulars() -> anyhow::Result<()> {
    // create temp dir and a small js file
    let td = tempdir()?;
    let file_path = td.path().join("one2.js");
    std::fs::write(&file_path, "import './two.js';")?;
    let file_path_s = file_path.to_string_lossy().to_string();

    let cli = Cli {
        files: vec![file_path_s.clone()],
        context: None,
        extensions: ".js".to_string(),

        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules|\\.git".to_string(),
        output: None,
        tree: false,
        circular: false,
        log: false,
        throw: false,
        tsconfig: None,

        exit_code: Some("circular:1".to_string()),
        progress: false,
        skip_dynamic_imports: false,
        take: None,
        watch: false,
        cache: false,
        no_cache: false,
        threads: Some(1),
    };

    // should not exit even though exit_code is specified because there are no circulars
    AnalysisRunner::run_analysis_once(&cli).await?;

    Ok(())
}

#[tokio::test]
async fn test_run_analysis_once_with_progress_none_and_ci_set() -> anyhow::Result<()> {
    let td = tempdir()?;
    let file_path = td.path().join("one3.js");
    std::fs::write(&file_path, "import './two.js';")?;
    let file_path_s = file_path.to_string_lossy().to_string();

    // set CI env to force show_progress -> false when progress is None
    std::env::set_var("CI", "1");

    let cli = Cli {
        files: vec![file_path_s.clone()],
        context: None,
        extensions: ".js".to_string(),

        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules|\\.git".to_string(),
        output: None,
        tree: false,
        circular: false,
        log: false,
        throw: false,
        tsconfig: None,

        exit_code: None,
        progress: false,
        skip_dynamic_imports: false,
        take: None,
        watch: false,
        cache: false,
        no_cache: false,
        threads: Some(1),
    };

    let res = AnalysisRunner::run_analysis_once(&cli).await?;
    let _ = res;

    // cleanup env
    std::env::remove_var("CI");

    Ok(())
}

#[tokio::test]
async fn test_display_analysis_results_with_circulars() -> anyhow::Result<()> {
    let result = AnalysisResult {
        entries: vec![],
        tree: DependencyTree::new(),
        circulars: vec![vec!["a.js".to_string(), "b.js".to_string()]],
    };

    let entries: Vec<String> = vec![];

    let cli = Cli {
        files: vec![],
        context: None,
        extensions: ".js".to_string(),

        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules|\\.git".to_string(),
        output: None,
        tree: false,
        circular: true,
        log: false,
        throw: false,
        tsconfig: None,

        exit_code: None,
        progress: false,
        skip_dynamic_imports: false,
        take: None,
        watch: false,
        cache: false,
        no_cache: false,
        threads: None,
    };

    AnalysisRunner::display_analysis_results(
        &result,
        &entries,
        std::time::Duration::from_secs(0),
        1,
        &cli,
    )
    .await?;

    Ok(())
}

#[tokio::test]
async fn test_run_analysis_once_with_real_file_and_progress() -> anyhow::Result<()> {
    // create temp dir and a small js file
    let td = tempdir()?;
    let file_path = td.path().join("one.js");
    std::fs::write(&file_path, "import './two.js';")?;
    let file_path_s = file_path.to_string_lossy().to_string();

    // create a cli that points to the file and enables progress
    let cli = Cli {
        files: vec![file_path_s.clone()],
        context: None,
        extensions: ".js".to_string(),

        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules|\\.git".to_string(),
        output: None,
        tree: false,
        circular: false,
        log: false,
        throw: false,
        tsconfig: None,

        exit_code: None,
        progress: true,
        skip_dynamic_imports: false,
        take: None,
        watch: false,
        cache: false,
        no_cache: false,
        threads: Some(1),
    };

    // should complete without error
    AnalysisRunner::run_analysis_once(&cli).await?;

    Ok(())
}
