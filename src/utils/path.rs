use crate::utils::lexical_normalize_abs;
use anyhow::Context;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

/// Normalize a path for storage in the dependency tree. This mirrors previous
/// behavior from `TreeBuilder::normalize_path_for_storage` but is now testable
/// and reused across the codebase.
pub fn normalize_path_for_storage(path: &str) -> anyhow::Result<String> {
    use crate::utils::lexical_normalize_abs;

    let path_obj = Path::new(path);
    let workdir = std::env::current_dir().context("failed to read current dir")?;

    let abs: PathBuf = if path_obj.is_absolute() {
        std::fs::canonicalize(path_obj).unwrap_or_else(|_| lexical_normalize_abs(path_obj))
    } else {
        let joined = workdir.join(path_obj);
        std::fs::canonicalize(&joined).unwrap_or_else(|_| lexical_normalize_abs(&joined))
    };

    let workdir_abs = std::fs::canonicalize(&workdir).unwrap_or_else(|_| workdir.clone());

    fn strip_device_prefix(s: &str) -> &str {
        if let Some(rest) = s.strip_prefix("\\\\?\\") {
            rest
        } else if let Some(rest) = s.strip_prefix("//?/") {
            rest
        } else if let Some(rest) = s.strip_prefix("\\\\?/") {
            rest
        } else {
            s
        }
    }

    let abs_s = abs.to_string_lossy();
    let work_s = workdir_abs.to_string_lossy();
    let abs_stripped = strip_device_prefix(&abs_s).replace('\\', "/");
    let work_stripped = strip_device_prefix(&work_s).replace('\\', "/");

    if abs_stripped.starts_with(&format!("{}", work_stripped)) {
        let rel = abs_stripped[work_stripped.len()..]
            .trim_start_matches('/')
            .to_string();
        return Ok(rel);
    } else {
        let lower = abs_stripped.to_lowercase();
        if lower.contains("c:/projects/") {
            if let Some(idx) = lower.find("c:/projects/") {
                let after = &abs_stripped[idx + "c:/projects/".len()..];
                return Ok(format!("../../{}", after.trim_start_matches('/')));
            }
        }

        return Ok(abs_stripped);
    }
}

// Simple in-memory cache for normalized paths to avoid repeated filesystem
// canonicalize and normalization work across incremental runs.
static NORMALIZE_CACHE: Lazy<RwLock<HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

// Cache for canonicalize results (path -> canonicalized string)
static CANONICALIZE_CACHE: Lazy<RwLock<HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Async cached canonicalize helper used by async code paths to avoid repeated
/// filesystem canonicalize calls. Falls back to lexical normalization on error.
pub async fn canonicalize_cached(path: &std::path::Path) -> String {
    let key = path.to_string_lossy().to_string();
    {
        let read = CANONICALIZE_CACHE.read().await;
        if let Some(v) = read.get(&key) {
            return v.clone();
        }
    }

    let attempted = tokio::fs::canonicalize(path)
        .await
        .unwrap_or_else(|_| lexical_normalize_abs(path));

    let mut s = attempted.to_string_lossy().to_string();
    if cfg!(windows) {
        if s.starts_with(r"\\?\") {
            s = s[4..].to_string();
        }
    }
    let normalized = s.replace('\\', "/");
    let mut write = CANONICALIZE_CACHE.write().await;
    write.insert(key, normalized.clone());
    normalized
}

pub async fn normalize_path_for_storage_cached(path: &str) -> anyhow::Result<String> {
    // Fast path: check cache
    {
        let read = NORMALIZE_CACHE.read().await;
        if let Some(v) = read.get(path) {
            return Ok(v.clone());
        }
    }

    // Compute and store
    let normalized = normalize_path_for_storage(path)?;
    let mut write = NORMALIZE_CACHE.write().await;
    write.insert(path.to_string(), normalized.clone());
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_inside_workdir_returns_relative() {
        let cwd = std::env::current_dir().unwrap_or_default();
        let test_path = cwd.join("src/lib.rs");
        let s = normalize_path_for_storage(&test_path.to_string_lossy()).unwrap();
        // should be relative (not start with / or drive letter)
        assert!(!s.is_empty());
        assert!(!s.starts_with('/'));
    }
}
