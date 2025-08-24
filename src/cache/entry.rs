use crate::types::Dependency;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub file_path: String,
    pub modified_time: SystemTime,
    pub content_hash: u64,
    pub dependencies: Vec<Dependency>,
    pub file_size: u64,
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub cached_files: usize,
    pub cached_tree_reuses: usize,
    pub hit_rate: f64,
}
