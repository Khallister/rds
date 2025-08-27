use crate::analyzer::tree::TreeBuilder;
use crate::types::ParseOptions;
use tempfile::tempdir;

#[tokio::test]
async fn test_build_dependency_tree_basic() -> anyhow::Result<()> {
    let mut builder = TreeBuilder::new()?;

    // create temp dir with two files: one imports the other
    let td = tempdir()?;
    let a = td.path().join("a.js");
    let b = td.path().join("b.js");
    std::fs::write(&b, "export const x = 1;")?;
    std::fs::write(&a, "import './b.js';")?;

    let entries = vec![a.to_string_lossy().to_string()];

    let mut options = ParseOptions::default();
    options.extensions = vec![".js".to_string()];
    options.extensions = vec![".js".to_string()];
    options.context = td.path().to_path_buf();

    let (tree, _threads) = builder.build_dependency_tree(&entries, &options).await?;

    // expect tree to contain an entry for a.js (path normalization varies across environments)
    assert!(tree.keys().any(|k| k.ends_with("a.js")));

    Ok(())
}

#[tokio::test]
async fn test_parse_file_static_excluded_returns_none() -> anyhow::Result<()> {
    let mut opts = crate::types::config::ParseOptions::default();
    // make include not match anything so file is treated as excluded
    opts.include = regex::Regex::new("^will_not_match$")?;

    let res = crate::analyzer::tree::parse::parse_file_static("somefile.js", &opts).await?;
    assert_eq!(res.0, "somefile.js");
    assert!(res.1.is_none());

    Ok(())
}

#[tokio::test]
async fn test_parse_single_file_deps_excluded_inserts_none() -> anyhow::Result<()> {
    let mut tb = crate::analyzer::tree::builder::TreeBuilder::new()?;

    let mut opts = crate::types::config::ParseOptions::default();
    // cause exclude to match the test filename
    opts.exclude = regex::Regex::new("exclude_me")?;

    let mut tree = crate::types::DependencyTree::new();
    crate::analyzer::tree::parse::parse_single_file_deps(
        tb.cache_mut(),
        "exclude_me.js",
        &opts,
        &mut tree,
    )
    .await?;

    assert!(tree.keys().any(|k| k.ends_with("exclude_me.js")));
    assert!(tree
        .get("exclude_me.js")
        .map(|v| v.is_none())
        .unwrap_or(false));

    Ok(())
}

#[tokio::test]
async fn test_parse_single_file_deps_no_parser_inserts_empty() -> anyhow::Result<()> {
    let mut tb = crate::analyzer::tree::builder::TreeBuilder::new()?;

    let mut opts = crate::types::config::ParseOptions::default();
    opts.context = std::env::current_dir().unwrap_or_default();

    let mut tree = crate::types::DependencyTree::new();

    // use an extension that's not handled by any parser
    crate::analyzer::tree::parse::parse_single_file_deps(
        tb.cache_mut(),
        "some.unknownext",
        &opts,
        &mut tree,
    )
    .await?;

    assert!(tree.keys().any(|k| k.ends_with("some.unknownext")));
    let val = tree.get("some.unknownext").unwrap();
    assert!(val.is_some() && val.as_ref().unwrap().is_empty());

    Ok(())
}

#[tokio::test]
async fn test_process_parsed_results_error_propagates() -> anyhow::Result<()> {
    let mut cache = crate::cache::FileCache::new(false);
    let resolver = crate::parser::ModuleResolver::new();

    let parsed_results = vec![Err(("bad.js".to_string(), anyhow::anyhow!("boom")))];

    let mut tree: crate::types::DependencyTree = crate::types::DependencyTree::new();
    let mut processed_files = std::collections::HashSet::new();
    let mut new_dependencies: Vec<String> = Vec::new();

    let opts = crate::types::config::ParseOptions::default();

    let res = crate::analyzer::tree::parse::process_parsed_results(
        &mut cache,
        &resolver,
        parsed_results,
        &mut tree,
        &mut processed_files,
        &mut new_dependencies,
        &opts,
    )
    .await;

    assert!(res.is_err());

    Ok(())
}

#[tokio::test]
async fn test_process_parsed_results_caches_when_enabled() -> anyhow::Result<()> {
    let mut cache = crate::cache::FileCache::new(true);
    let resolver = crate::parser::ModuleResolver::new();

    let td = tempdir()?;
    let f = td.path().join("cacheme.js");
    std::fs::write(&f, "console.log('x');")?;
    let f_path = std::fs::canonicalize(&f)?.to_string_lossy().to_string();

    let dep = crate::types::Dependency {
        issuer: f_path.clone(),
        request: "./other.js".to_string(),
        kind: crate::types::DependencyKind::StaticImport,
        id: None,
    };

    let parsed_results = vec![Ok((f_path.clone(), Some(vec![dep])))];

    let mut tree: crate::types::DependencyTree = crate::types::DependencyTree::new();
    let mut processed_files = std::collections::HashSet::new();
    let mut new_dependencies: Vec<String> = Vec::new();

    let mut opts = crate::types::config::ParseOptions::default();
    opts.extensions = vec![".js".to_string()];

    crate::analyzer::tree::parse::process_parsed_results(
        &mut cache,
        &resolver,
        parsed_results,
        &mut tree,
        &mut processed_files,
        &mut new_dependencies,
        &opts,
    )
    .await?;

    let cache_key = crate::utils::path::normalize_path_for_storage_cached(&f_path).await?;
    let cached = cache.get_cached_dependencies(&cache_key);
    assert!(cached.is_some());

    Ok(())
}

#[tokio::test]
async fn test_build_dependency_tree_incremental_cache_reuse() -> anyhow::Result<()> {
    let mut builder = TreeBuilder::new()?;

    // create temp dir and a file
    let td = tempdir()?;
    let one = td.path().join("one.js");
    std::fs::write(&one, "import './two.js';")?;

    let entries = vec![one.to_string_lossy().to_string()];

    let mut options = ParseOptions::default();
    options.extensions = vec![".js".to_string()];
    options.extensions = vec![".js".to_string()];
    options.cache_enabled = true;

    // run incremental once to populate last_analysis_cache
    let (_, _threads) = builder
        .build_dependency_tree_incremental(&entries, &options)
        .await?;
    let key = crate::utils::path::normalize_path_for_storage_cached(&entries[0]).await?;

    // run incremental again with the same single file; since nothing changed it should reuse cached tree
    let (inc_tree, _threads) = builder
        .build_dependency_tree_incremental(&entries, &options)
        .await?;
    assert!(inc_tree.contains_key(&key));

    Ok(())
}

#[tokio::test]
async fn test_build_dependency_tree_with_cached_results_resolves_deps() -> anyhow::Result<()> {
    let mut builder = TreeBuilder::new()?;

    // create temp dir with two files: cached 'a.js' that depends on './b.js'
    let td = tempdir()?;
    let a = td.path().join("a.js");
    let b = td.path().join("b.js");
    std::fs::write(&a, "console.log('a');")?;
    std::fs::write(&b, "console.log('b');")?;

    let a_path = std::fs::canonicalize(&a)?.to_string_lossy().to_string();
    let _b_path = std::fs::canonicalize(&b)?.to_string_lossy().to_string();

    // insert a cache entry for a.js with a dependency on './b.js'
    let cache_key = crate::utils::path::normalize_path_for_storage_cached(&a_path).await?;
    let dep = crate::types::Dependency {
        issuer: "a.js".to_string(),
        request: "./b.js".to_string(),
        kind: crate::types::DependencyKind::StaticImport,
        id: None,
    };
    builder
        .cache_mut()
        .cache_dependencies(&a_path, &cache_key, vec![dep])
        .await?;

    let entries = vec![a_path.clone()];

    let mut options = ParseOptions::default();
    options.extensions = vec![".js".to_string()];
    options.extensions = vec![".js".to_string()];
    options.context = td.path().to_path_buf();

    let (tree, _threads) = builder.build_dependency_tree(&entries, &options).await?;

    // a.js should be present in the tree; cache should have at least one cached file
    assert!(tree.keys().any(|k| k.ends_with("a.js")));
    let stats = builder.get_cache_stats();
    assert!(stats.cached_files >= 1);

    Ok(())
}

#[tokio::test]
async fn test_incremental_affected_set_limits_analysis() -> anyhow::Result<()> {
    let mut builder = TreeBuilder::new()?;

    // create temp dir with three files: a -> b -> c
    let td = tempdir()?;
    let a = td.path().join("a.js");
    let b = td.path().join("b.js");
    let c = td.path().join("c.js");
    std::fs::write(&c, "export const x = 1;")?;
    std::fs::write(&b, "import './c.js';")?;
    std::fs::write(&a, "import './b.js';")?;

    // Provide all files as entries so the full graph is discovered during the build
    let entries = vec![
        a.to_string_lossy().to_string(),
        b.to_string_lossy().to_string(),
        c.to_string_lossy().to_string(),
    ];

    let mut options = crate::types::config::ParseOptions::default();
    options.extensions = vec![".js".to_string()];
    options.context = td.path().to_path_buf();

    // Run a full build to populate reverse index
    let (_tree_full, _threads) = builder.build_dependency_tree(&entries, &options).await?;

    // Now simulate an incremental change in c.js and run incremental builder
    let changed = vec![c.to_string_lossy().to_string()];
    let (inc_tree, _threads) = builder
        .build_dependency_tree_incremental(&changed, &options)
        .await?;

    // Expect that a.js, b.js and c.js are present in the incremental tree
    assert!(inc_tree.keys().any(|k| k.ends_with("a.js")));
    assert!(inc_tree.keys().any(|k| k.ends_with("b.js")));
    assert!(inc_tree.keys().any(|k| k.ends_with("c.js")));

    Ok(())
}
use super::*;
use anyhow::Result;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_partition_cached_uses_cache() -> Result<()> {
    let mut tb = TreeBuilder::new()?;

    let temp_cached = NamedTempFile::new()?;
    let cached_path = temp_cached.path().to_string_lossy().to_string();
    tokio::fs::write(&cached_path, "console.log('cached');").await?;

    let temp_uncached = NamedTempFile::new()?;
    let uncached_path = temp_uncached.path().to_string_lossy().to_string();
    tokio::fs::write(&uncached_path, "console.log('uncached');").await?;

    let cache_key = crate::utils::path::normalize_path_for_storage_cached(&cached_path).await?;
    tb.cache_mut()
        .cache_dependencies(&cached_path, &cache_key, Vec::new())
        .await?;

    let opts = crate::types::config::ParseOptions::default();

    let (cached_results, files_to_parse) = partition::partition_cached(
        tb.cache_mut(),
        vec![cached_path.clone(), uncached_path.clone()],
        &opts,
    )
    .await?;

    assert!(cached_results.iter().any(|(p, _)| p == &cached_path));
    assert!(files_to_parse.iter().any(|p| p == &uncached_path));

    Ok(())
}

#[test]
fn test_expand_entries_directory_and_glob() -> Result<()> {
    use tempfile::tempdir;

    let td = tempdir()?;
    let dir_path = td.path();

    std::fs::create_dir_all(dir_path.join("subdir"))?;
    std::fs::write(dir_path.join("a.js"), "console.log(1);")?;
    std::fs::write(dir_path.join("b.ts"), "console.log(2);")?;
    std::fs::write(dir_path.join("subdir").join("c.js"), "console.log(3);")?;

    let mut opts = crate::types::config::ParseOptions::default();
    opts.context = dir_path.to_path_buf();

    let entries = vec![".".to_string()];
    let res = expand::expand_entries(&entries, &opts)?;

    assert!(res.iter().any(|s| s.ends_with("a.js")));
    assert!(res.iter().any(|s| s.ends_with("subdir/c.js")));

    let entries2 = vec!["*.ts".to_string()];
    let res2 = expand::expand_entries(&entries2, &opts)?;
    assert!(res2.iter().any(|s| s.ends_with("b.ts")));

    Ok(())
}

#[test]
fn test_expand_entries_missing_push_entry() -> Result<()> {
    let mut opts = crate::types::config::ParseOptions::default();
    opts.context = std::path::PathBuf::from(".");

    let entries = vec!["nonexistent.js".to_string()];
    let res = super::expand::expand_entries(&entries, &opts)?;

    assert!(res.iter().any(|s| s.ends_with("nonexistent.js")));

    Ok(())
}

#[test]
fn test_expand_entries_with_relative_context_uses_cwd() -> Result<()> {
    let mut opts = crate::types::config::ParseOptions::default();
    // use a relative context string to trigger env::current_dir join path
    opts.context = std::path::PathBuf::from("relative_test_dir");

    let entries = vec!["somefile.js".to_string()];
    let res = super::expand::expand_entries(&entries, &opts)?;

    // because the file doesn't exist, expand should return the original entry name
    assert!(res.iter().any(|s| s.ends_with("somefile.js")));

    Ok(())
}

#[test]
fn test_expand_entries_glob_dir_scans_directory() -> Result<()> {
    use tempfile::tempdir;

    let td = tempdir()?;
    let base = td.path();
    std::fs::create_dir_all(base.join("pack/inner"))?;
    std::fs::write(base.join("pack/x.js"), "console.log(1);")?;
    std::fs::write(base.join("pack/inner/y.js"), "console.log(2);")?;

    let mut opts = crate::types::config::ParseOptions::default();
    opts.context = base.to_path_buf();
    opts.extensions = vec![".js".to_string()];
    opts.extensions = vec![".js".to_string()];

    let entries = vec!["pack/*".to_string()];
    let res = super::expand::expand_entries(&entries, &opts)?;

    // should include both files discovered by scanning the directory
    assert!(res.iter().any(|s| s.ends_with("pack/x.js")));
    assert!(res.iter().any(|s| s.ends_with("pack/inner/y.js")));

    Ok(())
}

#[tokio::test]
async fn test_reverse_index_basic_merge_and_prune() -> anyhow::Result<()> {
    use crate::analyzer::tree::reverse_index::ReverseIndex;
    use crate::types::Dependency;

    // Build an initial full tree: a -> b -> c
    let mut full: crate::types::DependencyTree = crate::types::DependencyTree::new();
    full.insert(
        "a.js".to_string(),
        Some(vec![Dependency {
            issuer: "a.js".into(),
            request: "./b.js".into(),
            kind: crate::types::DependencyKind::StaticImport,
            id: Some("b.js".into()),
        }]),
    );
    full.insert(
        "b.js".to_string(),
        Some(vec![Dependency {
            issuer: "b.js".into(),
            request: "./c.js".into(),
            kind: crate::types::DependencyKind::StaticImport,
            id: Some("c.js".into()),
        }]),
    );
    full.insert("c.js".to_string(), Some(vec![]));

    let mut ri = ReverseIndex::from_tree(&full);

    // Initial affected set when c.js changes should include c,b,a
    let changed = vec!["c.js".to_string()];
    let affected = ri.compute_affected_set(&changed).await;
    assert!(affected.contains("c.js"));
    assert!(affected.contains("b.js"));
    assert!(affected.contains("a.js"));

    // Now simulate a partial update where b.js no longer depends on c.js
    let mut partial: crate::types::DependencyTree = crate::types::DependencyTree::new();
    partial.insert("b.js".to_string(), Some(vec![]));

    ri.merge_partial_into_full(&partial, &mut full);

    // After merge, affected set for c.js should not include b.js or a.js
    let affected2 = ri.compute_affected_set(&changed).await;
    assert!(affected2.contains("c.js"));
    assert!(!affected2.contains("b.js"));
    assert!(!affected2.contains("a.js"));

    // Now remove a.js entirely from full tree and prune
    full.remove("a.js");
    ri.prune(&full);
    // parents for c.js should not include a.js
    let parents = ri.get_parents("c.js");
    if let Some(p) = parents {
        assert!(!p.contains(&"a.js".to_string()));
    }

    Ok(())
}

#[test]
fn test_expand_entries_respects_include_exclude() -> Result<()> {
    use tempfile::tempdir;

    let td = tempdir()?;
    let dir_path = td.path();

    std::fs::write(dir_path.join("keep.js"), "")?;
    std::fs::write(dir_path.join("skip.js"), "")?;

    let mut opts = crate::types::config::ParseOptions::default();
    opts.context = dir_path.to_path_buf();
    opts.include = regex::Regex::new("keep")?;
    opts.exclude = regex::Regex::new("skip")?;

    let entries = vec![".".to_string()];
    let res = expand::expand_entries(&entries, &opts)?;

    assert!(res.iter().any(|s| s.ends_with("keep.js")));
    assert!(!res.iter().any(|s| s.ends_with("skip.js")));

    Ok(())
}

#[test]
fn test_expand_entries_excludes_node_modules() -> Result<()> {
    use tempfile::tempdir;

    let td = tempdir()?;
    let dir_path = td.path();

    std::fs::create_dir_all(dir_path.join("node_modules").join("pkg"))?;
    std::fs::write(dir_path.join("node_modules").join("pkg").join("x.js"), "")?;
    std::fs::write(dir_path.join("ok.js"), "")?;

    let mut opts = crate::types::config::ParseOptions::default();
    opts.context = dir_path.to_path_buf();

    let entries = vec![".".to_string()];
    let res = expand::expand_entries(&entries, &opts)?;

    assert!(res.iter().any(|s| s.ends_with("ok.js")));
    assert!(!res.iter().any(|s| s.contains("node_modules")));

    Ok(())
}

#[tokio::test]
async fn test_parse_file_static_no_parser_returns_empty_deps() -> anyhow::Result<()> {
    let opts = crate::types::config::ParseOptions::default();

    // extension `unknownext` is not handled by any built-in parser so we expect an empty deps vec
    let res = crate::analyzer::tree::parse::parse_file_static("somefile.unknownext", &opts).await?;
    assert_eq!(res.0, "somefile.unknownext");
    assert!(res.1.is_some());
    assert!(res.1.unwrap().is_empty());

    Ok(())
}

#[tokio::test]
async fn test_parse_files_batch_error_when_read_fails() -> anyhow::Result<()> {
    use std::sync::Arc;

    // register a toy parser for `.err` so parse_file_static will attempt to read the file
    struct ToyParser;
    impl crate::parser::Parser for ToyParser {
        fn parse_file(
            &self,
            _file_path: &str,
            _content: &str,
        ) -> anyhow::Result<Vec<crate::types::Dependency>> {
            Ok(vec![])
        }
        fn handled_extensions(&self) -> Vec<String> {
            vec!["err".to_string()]
        }
    }

    crate::parser::register_parser_for_extensions(vec!["err"], Arc::new(ToyParser));

    let opts = crate::types::config::ParseOptions::default();
    let results = crate::analyzer::tree::parse::parse_files_batch(
        vec!["missing_file.err".to_string()],
        &opts,
        2,
    )
    .await;

    assert_eq!(results.len(), 1);
    match &results[0] {
        Err((p, _)) => assert_eq!(p, "missing_file.err"),
        Ok(_) => panic!("expected error for missing file read"),
    }

    Ok(())
}

#[tokio::test]
async fn test_process_parsed_results_resolves_deps() -> anyhow::Result<()> {
    // use a disabled cache so we don't need files to satisfy cache checks
    let mut cache = crate::cache::FileCache::new(false);
    let resolver = crate::parser::ModuleResolver::new();

    let td = tempdir()?;
    let a = td.path().join("a.js");
    let b = td.path().join("b.js");
    std::fs::write(&a, "console.log('a');")?;
    std::fs::write(&b, "console.log('b');")?;

    let a_path = a.to_string_lossy().to_string();

    let dep = crate::types::Dependency {
        issuer: a_path.clone(),
        request: "./b.js".to_string(),
        kind: crate::types::DependencyKind::StaticImport,
        id: None,
    };

    let parsed_results = vec![Ok((a_path.clone(), Some(vec![dep])))];

    let mut tree: crate::types::DependencyTree = crate::types::DependencyTree::new();
    let mut processed_files = std::collections::HashSet::new();
    let mut new_dependencies: Vec<String> = Vec::new();

    let mut options = crate::types::config::ParseOptions::default();
    options.extensions = vec![".js".to_string()];
    options.context = td.path().to_path_buf();

    crate::analyzer::tree::parse::process_parsed_results(
        &mut cache,
        &resolver,
        parsed_results,
        &mut tree,
        &mut processed_files,
        &mut new_dependencies,
        &options,
    )
    .await?;

    assert!(tree.keys().any(|k| k.ends_with("a.js")));
    assert!(new_dependencies.iter().any(|s| s.ends_with("b.js")));

    Ok(())
}
