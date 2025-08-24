use super::*;
use std::path::PathBuf;
use crate::cli::Cli;
use notify::EventKind;
use tempfile::NamedTempFile;

#[test]
fn test_is_relevant_file_change() {
    assert!(is_relevant_file_change(&PathBuf::from("test.js")));
    assert!(is_relevant_file_change(&PathBuf::from("test.ts")));
    assert!(is_relevant_file_change(&PathBuf::from("test.vue")));
    assert!(!is_relevant_file_change(&PathBuf::from("test.txt")));
    assert!(!is_relevant_file_change(&PathBuf::from("README.md")));
}

#[test]
fn test_handle_exit_codes_no_circulars() {
    let result = exit_codes::handle_exit_codes("circular:1", &[]);
    assert!(result.is_ok());
}

#[test]
fn test_handle_exit_codes_invalid_format() {
    let result = exit_codes::handle_exit_codes("invalid_format", &[]);
    assert!(result.is_err());
}

#[test]
fn test_create_parse_options_from_cli_basic() {
    let cli = Cli {
        files: vec!["x".to_string()],
        context: None,
        extensions: ".js".to_string(),
        js: ".js".to_string(),
        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules".to_string(),
        output: None,
        tree: false,
        circular: false,
        warning: false,
        log: false,
        throw: false,
        tsconfig: None,
        transform: false,
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

    let opts = config::create_parse_options_from_cli(&cli).unwrap();
    assert!(opts.extensions.contains(&".js".to_string()));
}

#[test]
fn test_extract_relevant_file_changes_event() {
    use std::path::PathBuf;
    let ev = notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Any),
        paths: vec![PathBuf::from("a.js")],
        attrs: Default::default(),
    };

    let changes = extract_relevant_file_changes(&ev, &[]);
    assert_eq!(changes, vec!["a.js".to_string()]);
}

#[tokio::test]
async fn test_read_file_text_async_happy_path() {
    let mut nt = NamedTempFile::new().unwrap();
    use std::io::Write;
    writeln!(nt, "hello").unwrap();
    let path = nt.path().to_path_buf();
    let content = read_file_text_async(&path).await.unwrap();
    assert!(content.contains("hello"));
}

#[test]
fn test_lexical_normalize_handles_curdir() {
    use std::path::Path;
    let p = Path::new("a/./b");
    let normalized = lexical_normalize_abs(p);
    // Expect the curdir component to be removed; result should end with "a/b" (or Windows variant)
    let s = normalized.to_string_lossy().to_string();
    assert!(s.ends_with("a/b") || s.ends_with("a\\b"));
}

#[test]
fn test_extract_relevant_file_changes_other_kind() {
    use notify::EventKind;
    use std::path::PathBuf;

    let ev = notify::Event {
        kind: EventKind::Other,
        paths: vec![PathBuf::from("a.js")],
        attrs: Default::default(),
    };
    let changes = extract_relevant_file_changes(&ev, &[]);
    // should be empty because Other is not Create/Modify/Remove
    assert!(changes.is_empty());
}

#[test]
fn test_is_relevant_file_change_no_extension() {
    use std::path::PathBuf;
    assert!(!is_relevant_file_change(&PathBuf::from("LICENSE")));
}

#[test]
fn test_create_parse_options_from_cli_skip_dynamic_variants() {
    use crate::cli::SkipDynamicImportsArg;
    use crate::types::SkipDynamicImports;

    let mut cli = Cli {
        files: vec!["x".to_string()],
        context: None,
        extensions: ".js".to_string(),
        js: ".js".to_string(),
        filter: None,
        include: ".*".to_string(),
        exclude: "node_modules".to_string(),
        output: None,
        tree: false,
        circular: false,
        warning: false,
        log: false,
        throw: false,
        tsconfig: None,
        transform: false,
        exit_code: None,
        progress: None,
        detect_unused_files_from: None,
        skip_dynamic_imports: Some(SkipDynamicImportsArg::Tree),
        take: None,
        watch: false,
        cache: false,
        no_cache: false,
        threads: None,
    };

    let opts = config::create_parse_options_from_cli(&cli).unwrap();
    assert_eq!(opts.skip_dynamic_imports, SkipDynamicImports::Tree);

    cli.skip_dynamic_imports = Some(SkipDynamicImportsArg::Circular);
    let opts2 = config::create_parse_options_from_cli(&cli).unwrap();
    assert_eq!(opts2.skip_dynamic_imports, SkipDynamicImports::Circular);
}

#[test]
fn test_handle_exit_codes_invalid_number_and_unknown_case() {
    // invalid number should error
    let res = exit_codes::handle_exit_codes("circular:xyz", &[]);
    assert!(res.is_err());

    // unknown case should error
    let res2 = exit_codes::handle_exit_codes("other:1", &[]);
    assert!(res2.is_err());
}

#[test]
fn test_configure_thread_pool_none() {
    // calling with None should be a no-op and return Ok
    let res = threading::configure_thread_pool(None);
    assert!(res.is_ok());
}
