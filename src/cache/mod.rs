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
}

impl FileCache {
    /// Create a new file cache instance
    pub fn new(enabled: bool) -> Self {
        Self {
            cache: HashMap::new(),
            enabled,
            hits: 0,
            misses: 0,
        }
    }

    /// Check if a file is cached and up-to-date
    pub async fn is_cached(&mut self, file_path: &str) -> Result<bool> {
        if !self.enabled {
            return Ok(false);
        }

        let path = Path::new(file_path);
        if !path.exists() {
            return Ok(false);
        }

        // Get file metadata
        let metadata = tokio::fs::metadata(path).await?;
        let modified_time = metadata.modified()?;
        let file_size = metadata.len();

        // Check if we have this file in cache
        if let Some(entry) = self.cache.get(file_path) {
            // Check if the cached entry is still valid
            if entry.modified_time == modified_time && entry.file_size == file_size {
                return Ok(true);
            } else {
                // File has changed, remove from cache
                self.cache.remove(file_path);
            }
        }

        Ok(false)
    }

    /// Get cached dependencies for a file
    pub fn get_cached_dependencies(&mut self, file_path: &str) -> Option<Vec<Dependency>> {
        if !self.enabled {
            return None;
        }

        if let Some(entry) = self.cache.get(file_path) {
            self.hits += 1;
            Some(entry.dependencies.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    /// Cache the dependencies for a file
    pub async fn cache_dependencies(&mut self, file_path: &str, dependencies: Vec<Dependency>) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let path = Path::new(file_path);
        if !path.exists() {
            return Ok(());
        }

        // Get file metadata
        let metadata = tokio::fs::metadata(path).await?;
        let modified_time = metadata.modified()?;
        let file_size = metadata.len();

        // Read file content for hash calculation
        let content = tokio::fs::read_to_string(path).await?;
        let content_hash = calculate_hash(&content);

        let entry = CacheEntry {
            file_path: file_path.to_string(),
            modified_time,
            content_hash,
            dependencies,
            file_size,
        };

        self.cache.insert(file_path.to_string(), entry);
        Ok(())
    }

    /// Invalidate cache entry for a specific file
    pub fn invalidate(&mut self, file_path: &str) {
        self.cache.remove(file_path);
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
            hit_rate: if self.hits + self.misses > 0 {
                (self.hits as f64) / ((self.hits + self.misses) as f64) * 100.0
            } else {
                0.0
            },
        }
    }

    /// Get the number of cached files
    pub fn cached_files_count(&self) -> usize {
        self.cache.len()
    }

    /// Check if caching is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable caching
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }
}

/// Cache performance statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub cached_files: usize,
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
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_file_cache_basic() {
        let mut cache = FileCache::new(true);
        
        // Create a temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_string_lossy().to_string();
        
        // Write some content
        tokio::fs::write(&file_path, "console.log('test');").await.unwrap();
        
        // Should not be cached initially
        assert!(!cache.is_cached(&file_path).await.unwrap());
        assert!(cache.get_cached_dependencies(&file_path).is_none());
        
        // Cache some dependencies
        let deps = vec![];
        cache.cache_dependencies(&file_path, deps.clone()).await.unwrap();
        
        // Should be cached now
        assert!(cache.is_cached(&file_path).await.unwrap());
        assert_eq!(cache.get_cached_dependencies(&file_path), Some(deps));
        
        let stats = cache.get_stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.cached_files, 1);
    }

    #[tokio::test]
    async fn test_file_cache_invalidation() {
        let mut cache = FileCache::new(true);
        
        // Create a temporary file
        let mut temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_string_lossy().to_string();
        
        // Write initial content
        tokio::fs::write(&file_path, "console.log('test');").await.unwrap();
        
        // Cache dependencies
        let deps = vec![];
        cache.cache_dependencies(&file_path, deps).await.unwrap();
        assert!(cache.is_cached(&file_path).await.unwrap());
        
        // Modify the file
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        tokio::fs::write(&file_path, "console.log('modified');").await.unwrap();
        
        // Should no longer be cached due to modification
        assert!(!cache.is_cached(&file_path).await.unwrap());
    }

    #[tokio::test]
    async fn test_cache_disabled() {
        let mut cache = FileCache::new(false);
        
        // Create a temporary file
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path().to_string_lossy().to_string();
        
        tokio::fs::write(&file_path, "console.log('test');").await.unwrap();
        
        // Should never be cached when disabled
        assert!(!cache.is_cached(&file_path).await.unwrap());
        
        let deps = vec![];
        cache.cache_dependencies(&file_path, deps).await.unwrap();
        assert!(!cache.is_cached(&file_path).await.unwrap());
        assert!(cache.get_cached_dependencies(&file_path).is_none());
    }
}
