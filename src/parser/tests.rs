use super::*;
use crate::types::DependencyKind;

#[test]
fn test_js_parser_via_factory() {
    let parser = ParserFactory::get_parser_for_path("/path/to/file.js")
        .unwrap()
        .unwrap();
    let content = "import x from './b.js';\nconst y = require('./c.js');";
    let deps = parser.parse_file("/path/to/file.js", content).unwrap();
    assert!(deps
        .iter()
        .any(|d| d.request.ends_with("./b.js") && d.kind == DependencyKind::StaticImport));
    assert!(deps
        .iter()
        .any(|d| d.request.ends_with("./c.js") && d.kind == DependencyKind::CommonJS));
}

#[test]
fn test_vue_parser_via_factory() {
    let parser = ParserFactory::get_parser_for_path("/path/App.vue")
        .unwrap()
        .unwrap();
    let content =
        r#"<template><MyComp/></template><script>import something from './m.js'</script>"#;
    let deps = parser.parse_file("/path/App.vue", content).unwrap();
    assert!(deps
        .iter()
        .any(|d| d.request.contains("@/components/MyComp.vue") || d.request.ends_with("./m.js")));
}

#[test]
fn test_register_runtime_parser() {
    use std::sync::Arc;

    struct ToyParser;
    impl Parser for ToyParser {
        fn parse_file(
            &self,
            _file_path: &str,
            _content: &str,
        ) -> anyhow::Result<Vec<crate::types::Dependency>> {
            Ok(vec![crate::types::Dependency {
                issuer: "toy".into(),
                request: "toydep".into(),
                kind: crate::types::DependencyKind::StaticImport,
                id: None,
            }])
        }
        fn handled_extensions(&self) -> Vec<String> {
            vec!["foo".to_string()]
        }
    }

    // register toy parser for `.foo`
    register_parser_for_extensions(vec!["foo"], Arc::new(ToyParser));

    let p = ParserFactory::get_parser_for_path("/some/file.foo")
        .unwrap()
        .unwrap();
    let deps = p.parse_file("/some/file.foo", "").unwrap();
    assert!(deps
        .iter()
        .any(|d| d.request.contains("toydep") || d.issuer == "toy"));
}

#[tokio::test]
async fn test_runtime_parser_used_by_tree_builder() -> anyhow::Result<()> {
    use std::sync::Arc;
    use tempfile::tempdir;

    struct ToyParser;
    impl Parser for ToyParser {
        fn parse_file(
            &self,
            _file_path: &str,
            _content: &str,
        ) -> anyhow::Result<Vec<crate::types::Dependency>> {
            Ok(vec![crate::types::Dependency {
                issuer: "toy".into(),
                request: "toydep.js".into(),
                kind: crate::types::DependencyKind::StaticImport,
                id: None,
            }])
        }
        fn handled_extensions(&self) -> Vec<String> {
            vec!["foo".to_string()]
        }
    }

    // register toy parser for a custom extension `.foo` so we don't interfere with built-in JS tests
    register_parser_for_extensions(vec!["foo"], Arc::new(ToyParser));

    // create a temp dir + file
    let td = tempdir()?;
    let file_path = td.path().join("x.foo");
    std::fs::write(&file_path, "console.log('hello');")?;

    // prepare parse options with context set to tempdir and include .foo as a js-like extension
    let mut opts = crate::types::config::ParseOptions::default();
    opts.context = td.path().to_path_buf();
    opts.extensions.push(".foo".to_string());
    opts.js_extensions.push(".foo".to_string());

    let mut tb = crate::analyzer::tree::builder::TreeBuilder::new()?;

    let (tree, _threads) = tb
        .build_dependency_tree(&vec!["x.foo".to_string()], &opts)
        .await?;

    // because builder shortens paths when context != '.', the key should be "x.foo"
    let entry_opt = tree.get("x.foo");
    assert!(entry_opt.is_some(), "expected x.foo entry in tree");
    let deps_opt = entry_opt.unwrap();
    assert!(deps_opt.is_some(), "expected dependencies for x.foo");
    let deps = deps_opt.as_ref().unwrap();
    assert!(deps
        .iter()
        .any(|d| d.request.contains("toydep") || d.issuer == "toy"));

    Ok(())
}

#[test]
fn test_register_parser_uses_handled_extensions_and_lookup() {
    use std::sync::Arc;

    struct ToyParser2;
    impl Parser for ToyParser2 {
        fn parse_file(
            &self,
            _file_path: &str,
            _content: &str,
        ) -> anyhow::Result<Vec<crate::types::Dependency>> {
            Ok(vec![crate::types::Dependency {
                issuer: "toy2".into(),
                request: "toydep2".into(),
                kind: crate::types::DependencyKind::StaticImport,
                id: None,
            }])
        }
        fn handled_extensions(&self) -> Vec<String> {
            vec!["bar".to_string()]
        }
    }

    // register via the convenience function that queries handled_extensions()
    register_parser(Arc::new(ToyParser2));

    // lookup by extension should return our parser
    let p_opt = get_registered_parser_for_extension("bar");
    assert!(p_opt.is_some(), "expected registered parser for .bar");

    let p = p_opt.unwrap();
    let deps = p.parse_file("/some/file.bar", "").unwrap();
    assert!(deps
        .iter()
        .any(|d| d.request.contains("toydep2") || d.issuer == "toy2"));
}
