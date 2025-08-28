use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Calculates a 64-bit hash value for the given string content using the default hasher.
pub fn calculate_hash(content: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
}
