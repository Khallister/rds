use super::*;
use tempfile::NamedTempFile;

#[tokio::test]
async fn test_file_cache_basic() {
    let mut cache = FileCache::new(true);

    let temp_file = NamedTempFile::new().unwrap();
    let file_path = temp_file.path().to_string_lossy().to_string();

    tokio::fs::write(&file_path, "console.log('test');")
        .await
        .unwrap();

    let cache_key = file_path.clone();
    assert!(!cache.is_cached(&file_path, &cache_key).await.unwrap());
    assert!(cache.get_cached_dependencies(&cache_key).is_none());

    let deps = vec![];
    cache
        .cache_dependencies(&file_path, &cache_key, deps.clone())
        .await
        .unwrap();

    assert!(cache.is_cached(&file_path, &cache_key).await.unwrap());
    let got_deps = cache.get_cached_dependencies(&cache_key).unwrap();
    assert_eq!(got_deps.len(), deps.len());

    let stats = cache.get_stats();
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.cached_files, 1);
}

#[tokio::test]
async fn test_file_cache_invalidation() {
    let mut cache = FileCache::new(true);

    let temp_file = NamedTempFile::new().unwrap();
    let file_path = temp_file.path().to_string_lossy().to_string();

    tokio::fs::write(&file_path, "console.log('test');")
        .await
        .unwrap();

    let deps = vec![];
    let cache_key = file_path.clone();
    cache
        .cache_dependencies(&file_path, &cache_key, deps)
        .await
        .unwrap();
    assert!(cache.is_cached(&file_path, &cache_key).await.unwrap());

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    tokio::fs::write(&file_path, "console.log('modified');")
        .await
        .unwrap();

    let cache_key = file_path.clone();
    assert!(!cache.is_cached(&file_path, &cache_key).await.unwrap());
}

#[tokio::test]
async fn test_cache_disabled() {
    let mut cache = FileCache::new(false);

    let temp_file = NamedTempFile::new().unwrap();
    let file_path = temp_file.path().to_string_lossy().to_string();

    tokio::fs::write(&file_path, "console.log('test');")
        .await
        .unwrap();

    let cache_key = file_path.clone();
    assert!(!cache.is_cached(&file_path, &cache_key).await.unwrap());

    let deps = vec![];
    let cache_key = file_path.clone();
    cache
        .cache_dependencies(&file_path, &cache_key, deps)
        .await
        .unwrap();
    assert!(!cache.is_cached(&file_path, &cache_key).await.unwrap());
    assert!(cache.get_cached_dependencies(&cache_key).is_none());
}
