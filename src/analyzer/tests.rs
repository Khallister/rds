use super::*;
use crate::analyzer::DependencyAnalyzer;
use crate::types::ParseOptions;
use crate::types::{Dependency, DependencyKind, DependencyTree, SkipDynamicImports};

#[tokio::test]
async fn analyze_empty_entries_returns_empty_tree() {
    let mut analyzer =
        DependencyAnalyzer::new(ParseOptions::default()).expect("failed to create analyzer");
    let (result, _threads) = analyzer.analyze_files(&[]).await.expect("analysis failed");
    // empty entries should produce an empty tree and no circulars
    assert!(result.entries.is_empty());
    assert!(result.tree.is_empty());
    assert!(result.circulars.is_empty());
}

#[tokio::test]
async fn analyze_incremental_empty_returns_empty_tree() {
    let mut analyzer =
        DependencyAnalyzer::new(ParseOptions::default()).expect("failed to create analyzer");
    let (result, _threads) = analyzer
        .analyze_files_incremental(&[])
        .await
        .expect("analysis failed");
    assert!(result.entries.is_empty());
    assert!(result.tree.is_empty());
    assert!(result.circulars.is_empty());
}

#[test]
fn test_simple_cycle_detection() {
    let a = "A".to_string();
    let b = "B".to_string();

    let dep_ab = Dependency {
        issuer: a.clone(),
        request: "./b".to_string(),
        kind: DependencyKind::StaticImport,
        id: Some(b.clone()),
    };

    let dep_ba = Dependency {
        issuer: b.clone(),
        request: "./a".to_string(),
        kind: DependencyKind::StaticImport,
        id: Some(a.clone()),
    };

    let mut tree: DependencyTree = DependencyTree::new();
    tree.insert(a.clone(), Some(vec![dep_ab]));
    tree.insert(b.clone(), Some(vec![dep_ba]));

    let analyzer = CircularAnalyzer::new();
    let cycles = analyzer.find_circular_dependencies(&tree, &SkipDynamicImports::Never, None);

    assert!(!cycles.is_empty());
    // expect a 2-node cycle containing A and B
    assert!(
        cycles
            .iter()
            .any(|c| c.len() == 2 && c.contains(&a) && c.contains(&b))
    );
}

#[test]
fn test_skip_dynamic_imports_circular() {
    let a = "A".to_string();
    let b = "B".to_string();

    let dep_ab = Dependency {
        issuer: a.clone(),
        request: "./b".to_string(),
        kind: DependencyKind::DynamicImport,
        id: Some(b.clone()),
    };

    let dep_ba = Dependency {
        issuer: b.clone(),
        request: "./a".to_string(),
        kind: DependencyKind::StaticImport,
        id: Some(a.clone()),
    };

    let mut tree: DependencyTree = DependencyTree::new();
    tree.insert(a.clone(), Some(vec![dep_ab]));
    tree.insert(b.clone(), Some(vec![dep_ba]));

    let analyzer = CircularAnalyzer::new();
    // when skipping dynamic imports for circular detection we should not detect this cycle
    let cycles = analyzer.find_circular_dependencies(&tree, &SkipDynamicImports::Circular, None);
    assert!(cycles.is_empty());
}

#[test]
fn test_max_count_limit() {
    // create three independent 2-node cycles and ensure max_count limits results
    let mut tree: DependencyTree = DependencyTree::new();

    for (x, y) in &[("A", "B"), ("C", "D"), ("E", "F")] {
        let dep_xy = Dependency {
            issuer: x.to_string(),
            request: format!("./{}", y.to_lowercase()),
            kind: DependencyKind::StaticImport,
            id: Some(y.to_string()),
        };
        let dep_yx = Dependency {
            issuer: y.to_string(),
            request: format!("./{}", x.to_lowercase()),
            kind: DependencyKind::StaticImport,
            id: Some(x.to_string()),
        };
        tree.insert(x.to_string(), Some(vec![dep_xy]));
        tree.insert(y.to_string(), Some(vec![dep_yx]));
    }

    let analyzer = CircularAnalyzer::new();
    let cycles = analyzer.find_circular_dependencies(&tree, &SkipDynamicImports::Never, Some(1));
    assert_eq!(cycles.len(), 1);
}

#[test]
fn test_dependency_without_id_ignored() {
    // A has a dependency entry without id, B points back to A but since A's dep had no id
    // the traversal will not follow and no cycle should be reported.
    let a = "A".to_string();
    let b = "B".to_string();

    let dep_ab = Dependency {
        issuer: a.clone(),
        request: "./b".to_string(),
        kind: DependencyKind::StaticImport,
        id: None,
    };

    let dep_ba = Dependency {
        issuer: b.clone(),
        request: "./a".to_string(),
        kind: DependencyKind::StaticImport,
        id: Some(a.clone()),
    };

    let mut tree: DependencyTree = DependencyTree::new();
    tree.insert(a.clone(), Some(vec![dep_ab]));
    tree.insert(b.clone(), Some(vec![dep_ba]));

    let analyzer = CircularAnalyzer::new();
    let cycles = analyzer.find_circular_dependencies(&tree, &SkipDynamicImports::Never, None);
    assert!(cycles.is_empty());
}

#[test]
fn test_three_node_cycle_canonicalization() {
    let a = "A".to_string();
    let b = "B".to_string();
    let c = "C".to_string();

    let dep_ab = Dependency {
        issuer: a.clone(),
        request: "./b".to_string(),
        kind: DependencyKind::StaticImport,
        id: Some(b.clone()),
    };
    let dep_bc = Dependency {
        issuer: b.clone(),
        request: "./c".to_string(),
        kind: DependencyKind::StaticImport,
        id: Some(c.clone()),
    };
    let dep_ca = Dependency {
        issuer: c.clone(),
        request: "./a".to_string(),
        kind: DependencyKind::StaticImport,
        id: Some(a.clone()),
    };

    let mut tree: DependencyTree = DependencyTree::new();
    tree.insert(a.clone(), Some(vec![dep_ab]));
    tree.insert(b.clone(), Some(vec![dep_bc]));
    tree.insert(c.clone(), Some(vec![dep_ca]));

    let analyzer = CircularAnalyzer::new();
    let cycles = analyzer.find_circular_dependencies(&tree, &SkipDynamicImports::Never, None);
    assert!(
        cycles
            .iter()
            .any(|cyc| cyc.len() == 3 && cyc.contains(&a) && cyc.contains(&b) && cyc.contains(&c))
    );
}
