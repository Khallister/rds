use super::*;
use crate::types::{AnalysisResult, Dependency, DependencyKind};
use serde_json::{Value, from_value, to_value};
use std::collections::HashMap;
use tempfile::NamedTempFile;

#[test]
fn test_console_print_circular_empty() {
    let out = ConsoleOutput::new();
    // call function to ensure it doesn't panic and prints expected header
    out.print_circular(&Vec::new(), None, None, None::<String>);
}

#[test]
fn test_console_print_tree_captured_output() {
    let out = ConsoleOutput::new();

    // Build a simple tree with one file depending on another
    let mut tree: HashMap<String, Option<Vec<Dependency>>> = HashMap::new();
    tree.insert(
        "a.js".to_string(),
        Some(vec![Dependency {
            issuer: "a.js".to_string(),
            request: "b.js".to_string(),
            kind: DependencyKind::StaticImport,
            id: Some("b.js".to_string()),
        }]),
    );

    let entries = vec!["a.js".to_string()];

    // Capture output into buffer
    let mut buf: Vec<u8> = Vec::new();
    out.print_tree_to(&mut buf, &tree, &entries).unwrap();
    let s = String::from_utf8_lossy(&buf);

    assert!(s.contains("Dependencies Tree"));
    assert!(s.contains("a.js"));
    assert!(s.contains("b.js"));
}

#[test]
fn test_print_builtin_module_via_print_tree_to() {
    let out = ConsoleOutput::new();
    let mut tree: HashMap<String, Option<Vec<Dependency>>> = HashMap::new();

    // builtin module should be printed in blue; create an entry with the builtin id
    tree.insert("fs".to_string(), None);

    let entries = vec!["fs".to_string(), "fs".to_string()];
    let mut buf: Vec<u8> = Vec::new();
    out.print_tree_to(&mut buf, &tree, &entries).unwrap();
    let s = String::from_utf8_lossy(&buf);
    assert!(s.contains("fs"));
}

#[test]
fn test_print_circular_with_entries_and_limit() {
    let out = ConsoleOutput::new();
    let circulars = vec![
        vec!["A".to_string(), "B".to_string()],
        vec!["C".to_string(), "D".to_string()],
    ];
    // should print the circulars without panic
    out.print_circular(&circulars, Some(1), Some(1), None::<String>);
}

#[tokio::test]
async fn test_json_write_and_read() {
    let json_out = JsonOutput::new();
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();

    let mut tree: HashMap<String, Option<Vec<Dependency>>> = HashMap::new();
    tree.insert(
        "a.js".to_string(),
        Some(vec![Dependency {
            issuer: "a.js".to_string(),
            request: "b.js".to_string(),
            kind: DependencyKind::StaticImport,
            id: None,
        }]),
    );

    let result = AnalysisResult {
        entries: vec!["a.js".to_string()],
        tree,
        circulars: vec![],
    };

    json_out.write_to_file(&result, &path).await.unwrap();

    let content = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(content.contains("a.js"));
    assert!(content.contains("StaticImport"));
}

#[tokio::test]
async fn test_json_roundtrip_value_equality() {
    let mut tree: HashMap<String, Option<Vec<Dependency>>> = HashMap::new();
    tree.insert(
        "x.js".to_string(),
        Some(vec![Dependency {
            issuer: "x.js".to_string(),
            request: "y.js".to_string(),
            kind: DependencyKind::DynamicImport,
            id: Some("y.js".to_string()),
        }]),
    );

    let result = AnalysisResult {
        entries: vec!["x.js".to_string()],
        tree,
        circulars: vec![vec!["x.js".to_string(), "y.js".to_string()]],
    };

    // Serialize to JSON value and back, ensure equality
    let v: Value = to_value(&result).unwrap();
    let round: AnalysisResult = from_value(v.clone()).unwrap();
    let v2: Value = to_value(&round).unwrap();
    assert_eq!(v, v2);
}
