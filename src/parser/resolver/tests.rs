use super::ModuleResolver;
use anyhow::Result;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[tokio::test]
async fn test_resolve_ts_alias_wildcard_and_exact() -> Result<()> {
    let td = tempdir()?;
    let project = td.path();

    // create src/utils/foo.js which should be matched by @/* -> ./src/*
    let src_utils = project.join("src").join("utils");
    fs::create_dir_all(&src_utils)?;
    fs::write(src_utils.join("foo.js"), "module.exports = {};")?;

    // create lib/alias.js for exact mapping
    let lib_dir = project.join("lib");
    fs::create_dir_all(&lib_dir)?;
    fs::write(lib_dir.join("alias.js"), "module.exports = {};")?;

    // write tsconfig.json with wildcard and exact mappings
    let tsconfig = r#"
        {
            "compilerOptions": {
                "baseUrl": ".",
                "paths": {
                    "@/*": ["./src/*"],
                    "plain-alias": ["./lib/alias.js"]
                }
            }
        }
        "#;

    fs::write(project.join("tsconfig.json"), tsconfig)?;

    let r = ModuleResolver::new();

    // wildcard alias
    let resolved = r
        .resolve_module(project, "@/utils/foo", &vec![".js".to_string()])
        .await?;
    assert!(resolved.is_some());
    let rr = resolved.unwrap();
    assert!(rr.ends_with("src/utils/foo.js") || rr.ends_with("src\\utils\\foo.js"));

    // exact alias
    let resolved2 = r
        .resolve_module(project, "plain-alias", &vec![".js".to_string()])
        .await?;
    assert!(resolved2.is_some());
    let rr2 = resolved2.unwrap();
    assert!(rr2.ends_with("lib/alias.js") || rr2.ends_with("lib\\alias.js"));

    Ok(())
}

#[test]
fn test_normalize_path_and_pathbuf() {
    // platform-agnostic checks: calling impl helpers
    let normalized = ModuleResolver::normalize_path("/a/b/../c");
    assert!(normalized.ends_with("/a/c") || normalized.ends_with("\\a\\c"));

    let pb = Path::new("/a/b/../c");
    let normalized2 = ModuleResolver::normalize_pathbuf(pb);
    assert!(normalized2.ends_with("/a/c") || normalized2.ends_with("\\a\\c"));
}

#[tokio::test]
async fn test_resolve_module_builtin_and_append_suffix() -> Result<()> {
    let r = ModuleResolver::new();

    // builtin module should return as-is
    let res = r
        .resolve_module(".", "fs", &vec![".js".to_string()])
        .await?;
    assert_eq!(res.unwrap(), "fs");

    // create temp dir and file to test append_suffix
    let td = tempdir()?;
    let file_path = td.path().join("foo.js");
    std::fs::write(&file_path, "console.log('ok');")?;

    let req = file_path.to_string_lossy().to_string();
    let res2 = r.append_suffix(&req, &vec![".js".to_string()]).await?;
    assert!(res2.is_some());
    Ok(())
}

#[tokio::test]
async fn test_resolve_ts_alias_nested_baseurl() -> Result<()> {
    let td = tempdir()?;
    let project = td.path();

    // create sub/base/file.js to be resolved by pattern '@/nested/*' -> './sub/base/*'
    let target = project.join("sub").join("base");
    fs::create_dir_all(&target)?;
    fs::write(target.join("file.js"), "module.exports = {};")?;

    let tsconfig = r#"
        {
            "compilerOptions": {
                "baseUrl": ".",
                "paths": {
                    "@/nested/*": ["./sub/base/*"]
                }
            }
        }
        "#;
    fs::write(project.join("tsconfig.json"), tsconfig)?;

    let r = ModuleResolver::new();
    let resolved = r
        .resolve_module(project, "@/nested/file", &vec![".js".to_string()])
        .await?;
    assert!(resolved.is_some());
    let rr = resolved.unwrap();
    assert!(rr.ends_with("sub/base/file.js") || rr.ends_with("sub\\base\\file.js"));
    Ok(())
}

#[tokio::test]
async fn test_resolve_ts_alias_multiple_targets_fallback() -> Result<()> {
    let td = tempdir()?;
    let project = td.path();

    // create second target present, first target missing
    let present = project.join("src_present");
    fs::create_dir_all(&present)?;
    fs::write(present.join("file.js"), "module.exports = {};")?;

    let tsconfig = r#"
        {
            "compilerOptions": {
                "baseUrl": ".",
                "paths": {
                    "@/multi/*": ["./src_missing/*", "./src_present/*"]
                }
            }
        }
        "#;
    fs::write(project.join("tsconfig.json"), tsconfig)?;

    let r = ModuleResolver::new();
    let resolved = r
        .resolve_module(project, "@/multi/file", &vec![".js".to_string()])
        .await?;
    assert!(resolved.is_some());
    let rr = resolved.unwrap();
    assert!(rr.ends_with("src_present/file.js") || rr.ends_with("src_present\\file.js"));
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn test_resolve_ts_alias_leading_slash_target_unix() -> Result<()> {
    let td = tempdir()?;
    let project = td.path();

    // create an absolute directory and file that will be referenced with a leading-slash target
    let abs = project.join("abs_target");
    fs::create_dir_all(&abs)?;
    fs::write(abs.join("a.js"), "module.exports = {};")?;

    // on unix the absolute path starts with '/'
    let abs_path = abs.to_string_lossy().to_string();
    let tsconfig = format!(
        r#"
        {{
            "compilerOptions": {{
                "baseUrl": ".",
                "paths": {{
                    "abs/*": ["{}/*"]
                }}
            }}
        }}
        "#,
        abs_path
    );

    fs::write(project.join("tsconfig.json"), tsconfig)?;

    let r = ModuleResolver::new();
    let request = format!("abs/a");
    let resolved = r
        .resolve_module(project, &request, &vec![".js".to_string()])
        .await?;
    assert!(resolved.is_some());
    let rr = resolved.unwrap();
    assert!(rr.ends_with("abs_target/a.js") || rr.ends_with("abs_target\\a.js"));
    Ok(())
}

#[tokio::test]
async fn test_resolve_node_module_with_package_main() -> Result<()> {
    let td = tempdir()?;
    let project = td.path();
    // create node_modules/foo/package.json with {"main":"lib/index.js"}
    let nm = project.join("node_modules").join("foo");
    std::fs::create_dir_all(nm.join("lib"))?;
    std::fs::write(nm.join("lib").join("index.js"), "module.exports = {};")?;
    std::fs::write(nm.join("package.json"), r#"{"main":"lib/index.js"}"#)?;

    let r = ModuleResolver::new();
    let resolved = r
        .resolve_node_module(project, "foo", &vec![".js".to_string()])
        .await?;
    assert!(resolved.is_some());
    Ok(())
}

#[tokio::test]
async fn test_append_suffix_directory_index_resolution() -> Result<()> {
    let td = tempdir()?;
    let dir = td.path().join("mydir");
    std::fs::create_dir_all(&dir)?;
    // create index.js inside the directory
    std::fs::write(dir.join("index.js"), "module.exports = {};")?;

    let r = ModuleResolver::new();
    let req = dir.to_string_lossy().to_string();
    let resolved = r.append_suffix(&req, &vec![".js".to_string()]).await?;
    assert!(resolved.is_some());
    let rr = resolved.unwrap();
    assert!(rr.ends_with("index.js") || rr.ends_with("index\\index.js") || rr.contains("index.js"));
    Ok(())
}

#[tokio::test]
async fn test_resolve_relative_request_in_context() -> Result<()> {
    let td = tempdir()?;
    let project = td.path();
    let sub = project.join("sub");
    std::fs::create_dir_all(&sub)?;
    std::fs::write(sub.join("file.js"), "module.exports = {};")?;

    let r = ModuleResolver::new();
    let resolved = r
        .resolve_module(project, "./sub/file", &vec![".js".to_string()])
        .await?;
    assert!(resolved.is_some());
    let rr = resolved.unwrap();
    assert!(rr.ends_with("sub/file.js") || rr.ends_with("sub\\file.js"));
    Ok(())
}

#[tokio::test]
async fn test_resolve_node_module_uses_module_field() -> Result<()> {
    let td = tempdir()?;
    let project = td.path();
    // create node_modules/foo with package.json containing module field
    let nm = project.join("node_modules").join("foo");
    std::fs::create_dir_all(nm.join("lib"))?;
    std::fs::write(nm.join("lib").join("index.mjs"), "export default {};")?;
    std::fs::write(nm.join("package.json"), r#"{"module":"lib/index.mjs"}"#)?;

    let r = ModuleResolver::new();
    let resolved = r
        .resolve_node_module(project, "foo", &vec![".mjs".to_string(), ".js".to_string()])
        .await?;
    assert!(resolved.is_some());
    Ok(())
}

#[tokio::test]
async fn test_append_suffix_try_extensions() -> Result<()> {
    let td = tempdir()?;
    let file_base = td.path().join("basefile");
    // create basefile.ts
    std::fs::write(file_base.with_extension("ts"), "console.log('ts');")?;

    let r = ModuleResolver::new();
    let req = file_base.to_string_lossy().to_string();
    let resolved = r
        .append_suffix(&req, &vec![".ts".to_string(), ".js".to_string()])
        .await?;
    assert!(resolved.is_some());
    Ok(())
}

#[tokio::test]
async fn test_invalidate_paths_clears_caches() -> Result<()> {
    let td = tempdir()?;
    let project = td.path();

    // create a simple file and a package.json to exercise node resolution
    let nm = project.join("node_modules").join("pkg");
    std::fs::create_dir_all(nm.join("lib"))?;
    std::fs::write(nm.join("lib").join("index.js"), "module.exports = {};")?;
    std::fs::write(nm.join("package.json"), r#"{"main":"lib/index.js"}"#)?;

    let r = ModuleResolver::new();

    // Resolve once to populate caches
    let resolved = r
        .resolve_node_module(project, "pkg", &vec![".js".to_string()])
        .await?;
    assert!(resolved.is_some());

    // Now remove the file on disk to simulate external change
    std::fs::remove_file(nm.join("lib").join("index.js"))?;

    // Invalidate caches for the removed path
    let removed_path = nm
        .join("lib")
        .join("index.js")
        .to_string_lossy()
        .to_string();
    r.invalidate_paths(&vec![removed_path.clone()]).await;

    // After invalidation, resolver should no longer resolve the module (file missing)
    let resolved2 = r
        .resolve_node_module(project, "pkg", &vec![".js".to_string()])
        .await?;
    assert!(resolved2.is_none());

    Ok(())
}
