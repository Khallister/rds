use crate::types::Dependency;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use super::entry::CacheEntry;
use super::entry::CacheStats;
use crate::cache::hash::calculate_hash;

#[derive(Debug, Clone)]
pub struct FileCache {
    cache: HashMap<String, CacheEntry>,
    enabled: bool,
    hits: usize,
    misses: usize,
    cached_tree_reuses: usize,
}

impl FileCache {
    pub fn new(enabled: bool) -> Self {
        Self {
            cache: HashMap::new(),
            enabled,
            hits: 0,
            misses: 0,
            cached_tree_reuses: 0,
        }
    }

    pub async fn is_cached(&mut self, fs_path: &str, cache_key: &str) -> Result<bool> {
        if !self.enabled {
            return Ok(false);
        }

        let path = Path::new(fs_path);
        if !path.exists() {
            eprintln!("[cache] is_cached: fs_path does not exist: '{}'", fs_path);
            return Ok(false);
        }

        let metadata = tokio::fs::metadata(path).await?;
        let modified_time = metadata.modified()?;
        let file_size = metadata.len();

        if let Some(entry) = self.cache.get(cache_key) {
            if entry.modified_time == modified_time && entry.file_size == file_size {
                return Ok(true);
            } else {
                self.cache.remove(cache_key);
            }
        }

        Ok(false)
    }

    pub fn get_cached_dependencies(&mut self, cache_key: &str) -> Option<Vec<Dependency>> {
        if !self.enabled {
            return None;
        }

        if let Some(entry) = self.cache.get(cache_key) {
            self.hits += 1;
            Some(entry.dependencies.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    pub async fn cache_dependencies(
        &mut self,
        fs_path: &str,
        cache_key: &str,
        dependencies: Vec<Dependency>,
    ) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        let path = Path::new(fs_path);
        if !path.exists() {
            eprintln!(
                "[cache] cache_dependencies: fs_path does not exist, skipping: '{}'",
                fs_path
            );
            return Ok(());
        }

        let metadata = tokio::fs::metadata(path).await?;
        let modified_time = metadata.modified()?;
        let file_size = metadata.len();

        let content = crate::utils::read_file_text_async(path).await?;
        let content_hash = calculate_hash(&content);

        let entry = CacheEntry {
            file_path: fs_path.to_string(),
            modified_time,
            content_hash,
            dependencies,
            file_size,
        };

        self.cache.insert(cache_key.to_string(), entry);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }

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

        self.hits = 0;
        self.misses = 0;
        self.cached_tree_reuses = 0;

        stats
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.clear();
        }
    }

    pub fn incr_cached_tree_reuse(&mut self) {
        self.cached_tree_reuses += 1;
    }
}
