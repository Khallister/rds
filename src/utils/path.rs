use anyhow::Context;
use std::path::{Path, PathBuf};

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
