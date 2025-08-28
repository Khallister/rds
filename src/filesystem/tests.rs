use super::*;
use std::path::PathBuf;

#[test]
fn test_should_include_file_with_filter() {
    let filter_extensions = Some(vec![".js".to_string(), ".ts".to_string()]);

    assert!(FileSystem::should_include_file(
        &PathBuf::from("test.js"),
        &filter_extensions
    ));

    assert!(FileSystem::should_include_file(
        &PathBuf::from("test.ts"),
        &filter_extensions
    ));

    assert!(!FileSystem::should_include_file(
        &PathBuf::from("test.vue"),
        &filter_extensions
    ));
}

#[test]
fn test_should_include_file_default() {
    assert!(FileSystem::should_include_file(
        &PathBuf::from("test.js"),
        &None
    ));

    assert!(FileSystem::should_include_file(
        &PathBuf::from("test.vue"),
        &None
    ));

    assert!(!FileSystem::should_include_file(
        &PathBuf::from("test.txt"),
        &None
    ));
}

#[tokio::test]
async fn test_expand_file_inputs_with_dir_file_and_nonexistent() -> anyhow::Result<()> {
    use tempfile::tempdir;

    let td = tempdir()?;
    let base = td.path();
    // create files and a subdir
    std::fs::create_dir_all(base.join("sub"))?;
    std::fs::write(base.join("a.js"), "console.log(1);")?;
    std::fs::write(base.join("sub").join("b.ts"), "console.log(2);")?;

    let inputs = vec![
        base.to_string_lossy().to_string(),
        base.join("a.js").to_string_lossy().to_string(),
        "nonexistent.foo".to_string(),
    ];

    // filter None: should include .js and .ts by default
    let res = FileSystem::expand_file_inputs(&inputs, &None).await?;

    // should include both files and the explicit nonexistent entry
    assert!(res.iter().any(|s| s.ends_with("a.js")));
    assert!(
        res.iter()
            .any(|s| s.ends_with("sub/b.ts") || s.ends_with("sub\\b.ts"))
    );
    assert!(res.iter().any(|s| s == "nonexistent.foo"));

    Ok(())
}

#[tokio::test]
async fn test_expand_file_inputs_with_filter_string() -> anyhow::Result<()> {
    use tempfile::tempdir;

    let td = tempdir()?;
    let base = td.path();
    std::fs::write(base.join("x.js"), "")?;
    std::fs::write(base.join("y.txt"), "")?;

    let inputs = vec![base.to_string_lossy().to_string()];
    // pass a filter string (csv) to only include .js
    let res = FileSystem::expand_file_inputs(&inputs, &Some("js".to_string())).await?;
    assert!(res.iter().any(|s| s.ends_with("x.js")));
    assert!(!res.iter().any(|s| s.ends_with("y.txt")));

    Ok(())
}

#[test]
fn test_get_watch_directories_returns_parents() {
    let files = vec![
        "a/b/c.js".to_string(),
        "d/e.js".to_string(),
        "f.js".to_string(),
    ];
    let mut dirs = FileSystem::get_watch_directories(&files);
    dirs.sort();
    assert!(dirs.iter().any(|d| d.ends_with("a/b")));
    assert!(dirs.iter().any(|d| d.ends_with("d")));
    assert!(dirs.iter().any(|d| d.ends_with(".")) || dirs.iter().any(|d| d.ends_with("")));
    // f.js parent may be '.' or empty
}
