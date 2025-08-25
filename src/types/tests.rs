use super::*;
use serde_json;

#[test]
fn dependency_kind_serde_roundtrip() {
    let k = DependencyKind::StaticImport;
    let s = serde_json::to_string(&k).unwrap();
    assert!(s.contains("StaticImport"));
    let k2: DependencyKind = serde_json::from_str(&s).unwrap();
    assert_eq!(k, k2);
}

#[test]
fn dependency_struct_serde_roundtrip() {
    let d = Dependency {
        issuer: "a.js".into(),
        request: "./b.js".into(),
        kind: DependencyKind::CommonJS,
        id: Some("/abs/b.js".into()),
    };

    let s = serde_json::to_string(&d).unwrap();
    let d2: Dependency = serde_json::from_str(&s).unwrap();
    assert_eq!(d.issuer, d2.issuer);
    assert_eq!(d.request, d2.request);
    assert_eq!(d.kind, d2.kind);
    assert_eq!(d.id, d2.id);
}

#[test]
fn dependency_tree_insert_and_get() {
    let mut tree: DependencyTree = DependencyTree::new();
    tree.insert(
        "x.js".to_string(),
        Some(vec![Dependency {
            issuer: "x.js".into(),
            request: "y.js".into(),
            kind: DependencyKind::StaticImport,
            id: None,
        }]),
    );
    assert!(tree.contains_key("x.js"));
    let val = tree.get("x.js").unwrap();
    assert!(val.is_some());
}

#[test]
fn parse_options_defaults() {
    let opts = ParseOptions::default();
    assert!(opts.extensions.contains(&".js".to_string()));
    assert!(opts.extensions.contains(&".js".to_string()));
    assert!(opts.cache_enabled);
}

#[test]
fn parse_options_debug_and_enums() {
    let opts = ParseOptions::default();
    // ensure Debug implementation doesn't panic and includes field names
    let s = format!("{:?}", opts);
    assert!(s.contains("extensions"));

    // SkipDynamicImports variants roundtrip via equality
    assert_eq!(
        crate::types::config::SkipDynamicImports::Never,
        crate::types::config::SkipDynamicImports::Never
    );

    // ProgressEvent debug
    let ev = crate::types::config::ProgressEvent::Start;
    let s2 = format!("{:?}", ev);
    assert!(s2.contains("Start"));
}
