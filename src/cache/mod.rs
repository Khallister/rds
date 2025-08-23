use std::collections::HashMap;
use std::path::Path;
use std::time::SystemTime;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::types::Dependency;

/// Represents a cached file entry with its dependencies and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// File path (absolute)
    pub file_path: String,
    /// Last modification time
    pub modified_time: SystemTime,
    /// File content hash for integrity checking
    pub content_hash: u64,
    /// Parsed dependencies from this file
    pub dependencies: Vec<Dependency>,
    /// File size in bytes
    pub file_size: u64,
}

/// File cache manager for dependency analysis results
#[derive(Debug, Clone)]
pub struct FileCache {
    /// In-memory cache of parsed files
    cache: HashMap<String, CacheEntry>,
    /// Whether caching is enabled
    enabled: bool,
    /// Cache hit statistics
    hits: usize,
    misses: usize,
    /// Number of times a cached tree/result was reused (incremented by caller)
    cached_tree_reuses: usize,
}

impl FileCache {
    /// Create a new file cache instance
    pub fn new(enabled: bool) -> Self {
        Self {
            cache: HashMap::new(),
            enabled,
            hits: 0,
            misses: 0,
            cached_tree_reuses: 0,
        }
    }

    /// Check if a file is cached and up-to-date
    /// Check if a file is cached and up-to-date.
    /// `fs_path` is the actual filesystem path used to read metadata.
    /// `cache_key` is the storage-normalized key used to index the internal cache map.
    pub async fn is_cached(&mut self, fs_path: &str, cache_key: &str) -> Result<bool> {
        if !self.enabled {
            return Ok(false);
        }

        // Debug: show lookup attempt
        eprintln!("[cache] is_cached lookup -> cache_key='{}' fs_path='{}' enabled={}", cache_key, fs_path, self.enabled);

        let path = Path::new(fs_path);
        if !path.exists() {
            eprintln!("[cache] is_cached: fs_path does not exist: '{}'", fs_path);
            return Ok(false);
        }

        // Get file metadata
        let metadata = tokio::fs::metadata(path).await?;
        let modified_time = metadata.modified()?;
        let file_size = metadata.len();

        // Check if we have this file in cache using cache_key
        if let Some(entry) = self.cache.get(cache_key) {
            eprintln!("[cache] is_cached: found entry for key='{}' (cached_file='{}')", cache_key, entry.file_path);
            // Check if the cached entry is still valid
            if entry.modified_time == modified_time && entry.file_size == file_size {
                eprintln!("[cache] is_cached: entry valid (mtime and size match)");
                return Ok(true);
            } else {
                eprintln!("[cache] is_cached: entry invalid, removing from cache (mtime or size changed)");
                // File has changed, remove from cache
                self.cache.remove(cache_key);
            }
        }

        Ok(false)
    }

    /// Get cached dependencies for a file
    /// Get cached dependencies by storage-normalized cache key.
    pub fn get_cached_dependencies(&mut self, cache_key: &str) -> Option<Vec<Dependency>> {
        if !self.enabled {
            return None;
        }

        eprintln!("[cache] get_cached_dependencies lookup -> cache_key='{}'", cache_key);
        if let Some(entry) = self.cache.get(cache_key) {
            self.hits += 1;
            eprintln!("[cache] get_cached_dependencies: hit for key='{}' (file='{}')", cache_key, entry.file_path);
            Some(entry.dependencies.clone())
        } else {
            self.misses += 1;
            eprintln!("[cache] get_cached_dependencies: miss for key='{}'", cache_key);
            None
        }
    }

    /// Cache the dependencies for a file
    /// Cache the dependencies for a file.
    /// `fs_path` is the filesystem path used to read the file; `cache_key` is the storage-normalized key to store under.
    pub async fn cache_dependencies(&mut self, fs_path: &str, cache_key: &str, dependencies: Vec<Dependency>) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        eprintln!("[cache] cache_dependencies store -> cache_key='{}' fs_path='{}'", cache_key, fs_path);

        let path = Path::new(fs_path);
        if !path.exists() {
            eprintln!("[cache] cache_dependencies: fs_path does not exist, skipping: '{}'", fs_path);
            return Ok(());
        }

        // Get file metadata
        let metadata = tokio::fs::metadata(path).await?;
        let modified_time = metadata.modified()?;
        let file_size = metadata.len();

        // Read file content for hash calculation
        let content = crate::utils::read_file_text_async(path).await?;
        let content_hash = calculate_hash(&content);

        let entry = CacheEntry {
            file_path: fs_path.to_string(),
            modified_time,
            content_hash,
            dependencies,
            file_size,
        };

    eprintln!("[cache] cache_dependencies: inserting entry for key='{}' (file='{}')", cache_key, fs_path);
    self.cache.insert(cache_key.to_string(), entry);
        Ok(())
    }

    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Get cache statistics
    pub fn get_stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            cached_files: self.cache.len(),
            cached_tree_reuses: self.cached_tree_reuses,
            hit_rate: if self.hits + self.misses > 0 {
                (self.hits as f64) / ((self.hits + self.misses) as f64) * 100.0
            } else {
                0.0
            },
        }
    }
    
    /// Get cache statistics since last call to this method (incremental stats)
    pub fn get_incremental_stats(&mut self) -> CacheStats {
        let stats = CacheStats {
            hits: self.hits,
            misses: self.misses,
            cached_files: self.cache.len(),
            cached_tree_reuses: self.cached_tree_reuses,
            hit_rate: if self.hits + self.misses > 0 {
                (self.hits as f64) / ((self.hits + self.misses) as f64) * 100.0
            } else {
                0.0
            },
        };
        
        // Reset counters for next measurement
        self.hits = 0;
        self.misses = 0;
        self.cached_tree_reuses = 0;
        
        stats
    }

    /// Enable or disable caching
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    /// Increment cached-tree reuse counter (called by TreeBuilder when last_analysis_cache is reused)
    pub fn incr_cached_tree_reuse(&mut self) {
        self.cached_tree_reuses += 1;
    }
}

/// Cache performance statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub cached_files: usize,
    pub cached_tree_reuses: usize,
    pub hit_rate: f64,
}

/// Calculate a simple hash of file content for integrity checking
fn calculate_hash(content: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_file_cache_basic() {
        let mut cache = FileCache::new(true);
        
        // Create a temporary file
    let temp_file = NamedTempFile::new().unwrap();
    let file_path = temp_file.path().to_string_lossy().to_string();
        
        // Write some content
        tokio::fs::write(&file_path, "console.log('test');").await.unwrap();
        
    // Should not be cached initially
    let cache_key = file_path.clone();
    assert!(!cache.is_cached(&file_path, &cache_key).await.unwrap());
    assert!(cache.get_cached_dependencies(&cache_key).is_none());

    // Cache some dependencies
    let deps = vec![];
    cache.cache_dependencies(&file_path, &cache_key, deps.clone()).await.unwrap();

    // Should be cached now
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
        
        // Create a temporary file
    let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_string_lossy().to_string();
        
        // Write initial content
        tokio::fs::write(&file_path, "console.log('test');").await.unwrap();
        
        // Cache dependencies
        let deps = vec![];
    let cache_key = file_path.clone();
    cache.cache_dependencies(&file_path, &cache_key, deps).await.unwrap();
    assert!(cache.is_cached(&file_path, &cache_key).await.unwrap());
        
        // Modify the file
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        tokio::fs::write(&file_path, "console.log('modified');").await.unwrap();
        
        // Should no longer be cached due to modification
    let cache_key = file_path.clone();
    assert!(!cache.is_cached(&file_path, &cache_key).await.unwrap());
    }

    #[tokio::test]
    async fn test_cache_disabled() {
        let mut cache = FileCache::new(false);
        
        // Create a temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_string_lossy().to_string();
        
        tokio::fs::write(&file_path, "console.log('test');").await.unwrap();
        
        // Should never be cached when disabled
    let cache_key = file_path.clone();
    assert!(!cache.is_cached(&file_path, &cache_key).await.unwrap());
    
    let deps = vec![];
    let cache_key = file_path.clone();
    cache.cache_dependencies(&file_path, &cache_key, deps).await.unwrap();
    assert!(!cache.is_cached(&file_path, &cache_key).await.unwrap());
    assert!(cache.get_cached_dependencies(&cache_key).is_none());
    }
}
