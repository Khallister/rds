use super::JavaScriptParser;
use super::VueParser;
use crate::types::DependencyKind;
use anyhow::Result;

#[test]
fn test_handled_extensions_listed() {
    let p = JavaScriptParser::new().unwrap();
    let exts = p.handled_extensions();
    assert!(exts.contains(&"js".to_string()));
    assert!(exts.contains(&"ts".to_string()));
}

#[test]
fn test_parse_imports_and_requires_and_exports() {
    let p = JavaScriptParser::new().unwrap();
    let content = r#"
        import x from './b.js';
        const y = require('./c.js');
        import('./d.js');
        export { foo } from './e.js';
    "#;

    let deps = p.parse_file("/tmp/a.js", content).unwrap();
    assert!(deps.iter().any(|d| d.request.ends_with("./b.js") && d.kind == DependencyKind::StaticImport));
    assert!(deps.iter().any(|d| d.request.ends_with("./c.js") && d.kind == DependencyKind::CommonJS));
    assert!(deps.iter().any(|d| d.request.ends_with("./d.js") && d.kind == DependencyKind::DynamicImport));
    assert!(deps.iter().any(|d| d.request.ends_with("./e.js") && d.kind == DependencyKind::StaticExport));
}

#[test]
fn test_parse_ignores_commented_imports() {
    let p = JavaScriptParser::new().unwrap();
    let content = r#"
        // import './nope.js'
        const s = "import './real.js'";
        /* import './also_nope.js' */
        import './real.js';
    "#;

    let deps = p.parse_file("/tmp/a.js", content).unwrap();
    // only one real import should be found
    assert_eq!(deps.iter().filter(|d| d.request.ends_with("./real.js")).count(), 1);
}

#[test]
fn test_vue_handled_extensions_listed() {
    let p = VueParser::new().unwrap();
    let exts = p.handled_extensions();
    assert!(exts.contains(&"vue".to_string()));
}

#[test]
fn test_parse_vue_script_and_setup() {
    let p = VueParser::new().unwrap();
    let content = r#"
        <script>
            import a from './a.js';
        </script>
        <script setup>
            const b = require('./b.js');
        </script>
    "#;

    let deps = p.parse_file("/tmp/comp.vue", content).unwrap();
    assert!(deps.iter().any(|d| d.request.ends_with("./a.js") && d.kind == DependencyKind::VueScript));
    assert!(deps.iter().any(|d| d.request.ends_with("./b.js") && d.kind == DependencyKind::VueScript));
}

#[test]
fn test_parse_vue_template_components() {
    let p = VueParser::new().unwrap();
    let content = r#"
        <template>
            <div>
                <MyComponent />
                <lowercase />
            </div>
        </template>
    "#;

    let deps = p.parse_file("/tmp/comp.vue", content).unwrap();
    // only the PascalCase component should be picked up
    assert!(deps.iter().any(|d| d.request.ends_with("@/components/MyComponent.vue") && d.kind == DependencyKind::VueTemplate));
    assert!(!deps.iter().any(|d| d.request.contains("lowercase") && d.kind == DependencyKind::VueTemplate));
}

#[test]
fn test_parse_vue_style_imports() {
    let p = VueParser::new().unwrap();
    let content = r#"
        <style>
            @import './styles/a.css';
            @import "./styles/b.scss";
        </style>
    "#;

    let deps = p.parse_file("/tmp/comp.vue", content).unwrap();
    assert!(deps.iter().any(|d| d.request.ends_with("./styles/a.css") && d.kind == DependencyKind::VueStyle));
    assert!(deps.iter().any(|d| d.request.ends_with("./styles/b.scss") && d.kind == DependencyKind::VueStyle));
}

#[test]
fn test_parse_all_import_variants() -> Result<()> {
    let p = JavaScriptParser::new()?;
    let content = r#"
        import def from 'mod1';
        import { named } from "mod2";
        const x = require('mod3');
        const y = require("mod4");
        import('mod5');
        export { a } from 'mod6';
    "#;

    let deps = p.parse_file("file.js", content)?;
    let mut kinds: Vec<String> = deps.iter().map(|d| format!("{:?}", d.kind)).collect();
    kinds.sort();

    assert!(kinds.iter().any(|k| k.contains("StaticImport")));
    assert!(kinds.iter().any(|k| k.contains("CommonJS")));
    assert!(kinds.iter().any(|k| k.contains("DynamicImport")));
    assert!(kinds.iter().any(|k| k.contains("StaticExport")));

    let requests: Vec<String> = deps.into_iter().map(|d| d.request).collect();
    assert!(requests.contains(&"mod1".to_string()));
    assert!(requests.contains(&"mod3".to_string()));
    assert!(requests.contains(&"mod5".to_string()));
    assert!(requests.contains(&"mod6".to_string()));

    Ok(())
}

#[test]
fn test_strings_and_template_literals_scrub_import_like_keywords() -> Result<()> {
    let p = JavaScriptParser::new()?;
    let content = r#"
        const a = "this string mentions import 'nope' and require('nope')";
        const b = 'also export from "nope" inside single quotes';
        const t = `template with import('nope') and require('nope')`;
        // and one real import below
        import real from 'real-mod';
    "#;

    let deps = p.parse_file("strings.js", content)?;
    let requests: Vec<String> = deps.into_iter().map(|d| d.request).collect();

    assert_eq!(requests, vec!["real-mod".to_string()]);
    Ok(())
}

#[test]
fn test_escaped_quotes_inside_strings_and_slash_default_branch() -> Result<()> {
    let p = JavaScriptParser::new()?;
    let content = r#"
        const s = "a string with escaped \" import 'not' \" and more";
        const p = "/a"; // this should trigger the '/' default branch in remove_comments
        import real2 from 'r2';
    "#;

    let deps = p.parse_file("esc.js", content)?;
    let requests: Vec<String> = deps.into_iter().map(|d| d.request).collect();
    assert!(requests.contains(&"r2".to_string()));
    assert!(!requests.contains(&"not".to_string()));

    Ok(())
}

#[test]
fn test_namespace_and_side_effect_imports_and_export_all() -> Result<()> {
    let p = JavaScriptParser::new()?;
    let content = r#"
        import * as ns from 'ns-mod';
        import 'side-effect';
        export * from "export-all";
    "#;

    let deps = p.parse_file("ns.js", content)?;
    let requests: Vec<String> = deps.into_iter().map(|d| d.request).collect();
    assert!(requests.contains(&"ns-mod".to_string()));
    assert!(requests.contains(&"side-effect".to_string()));
    assert!(requests.contains(&"export-all".to_string()));
    Ok(())
}

#[test]
fn test_require_and_dynamic_import_variants() -> Result<()> {
    let p = JavaScriptParser::new()?;
    let content = r#"
        const a = require ( 'req1' );
        const b = require(  "req2" );
        import(  'dyn1' );
        import ("dyn2");
    "#;

    let deps = p.parse_file("req.js", content)?;
    let mut requests: Vec<String> = deps.into_iter().map(|d| d.request).collect();
    requests.sort();
    assert!(requests.contains(&"req1".to_string()));
    assert!(requests.contains(&"req2".to_string()));
    assert!(requests.contains(&"dyn1".to_string()));
    assert!(requests.contains(&"dyn2".to_string()));
    Ok(())
}

#[test]
fn test_complex_escaped_string_handling() -> Result<()> {
    let p = JavaScriptParser::new()?;
    // string contains multiple escaped sequences, backslashes and quotes
    let content = r#"
        const s = "first \" inner \\ still inner import 'no' end";
        const t = `templ \` value import('no') more`;
        import found from 'found-mod';
    "#;

    let deps = p.parse_file("esc2.js", content)?;
    let requests: Vec<String> = deps.into_iter().map(|d| d.request).collect();
    // Only the real import should be present
    assert_eq!(requests, vec!["found-mod".to_string()]);
    Ok(())
}

#[test]
fn test_multiple_imports_and_requires() -> Result<()> {
    let p = JavaScriptParser::new()?;
    let content = r#"
        import a from 'a1';
        import b from 'b1';
        const c = require('c1');
        const d = require('d1');
        import('dyn1');
        import('dyn2');
        export { x } from 'exp1';
        export * from "exp2";
    "#;

    let mut deps = p.parse_file("many.js", content)?;
    // sort to make assertions order-independent
    deps.sort_by(|a, b| a.request.cmp(&b.request));

    let requests: Vec<String> = deps.into_iter().map(|d| d.request).collect();
    assert!(requests.contains(&"a1".to_string()));
    assert!(requests.contains(&"b1".to_string()));
    assert!(requests.contains(&"c1".to_string()));
    assert!(requests.contains(&"d1".to_string()));
    assert!(requests.contains(&"dyn1".to_string()));
    assert!(requests.contains(&"dyn2".to_string()));
    assert!(requests.contains(&"exp1".to_string()));
    assert!(requests.contains(&"exp2".to_string()));
    Ok(())
}

#[test]
fn test_parse_with_pathbuf_issuer() -> Result<()> {
    use std::path::PathBuf;
    let p = JavaScriptParser::new()?;
    let content = "import m from './mod.js';";
    let path = PathBuf::from("/tmp/somepath/a.js");
    let deps = p.parse_file(path.clone(), content)?;
    assert!(!deps.is_empty());
    // ensure issuer contains filename
    assert!(deps[0].issuer.ends_with("a.js"));
    Ok(())
}

#[test]
fn test_parse_empty_returns_empty() -> Result<()> {
    let p = JavaScriptParser::new()?;
    let content = "// no imports here";
    let deps = p.parse_file("empty.js", content)?;
    assert!(deps.is_empty());
    Ok(())
}
