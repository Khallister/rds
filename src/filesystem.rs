//! File system operations and file discovery logic.
//!
//! This module handles file discovery, directory scanning, and file filtering
//! operations used by the dependency analyzer.

use anyhow::Result;
use std::path::Path;
use tokio::fs;

pub struct FileSystem;

impl FileSystem {
    pub async fn expand_file_inputs(
        inputs: &[String],
        filter: &Option<String>,
    ) -> Result<Vec<String>> {
        let mut expanded_files = Vec::new();

        let filter_extensions: Option<Vec<String>> = filter.as_ref().map(|f| {
            f.split(',')
                .map(|ext| {
                    let ext = ext.trim();
                    if ext.starts_with('.') {
                        ext.to_string()
                    } else {
                        format!(".{}", ext)
                    }
                })
                .collect()
        });

        for input in inputs {
            let path = Path::new(input);

            if path.is_dir() {
                let dir_files = Self::scan_directory(path, &filter_extensions).await?;
                expanded_files.extend(dir_files);
            } else if path.is_file() {
                if Self::should_include_file(path, &filter_extensions) {
                    expanded_files.push(input.clone());
                }
            } else {
                expanded_files.push(input.clone());
            }
        }

        expanded_files.sort();
        expanded_files.dedup();

        Ok(expanded_files)
    }

    fn scan_directory<'a>(
        dir: &'a Path,
        filter_extensions: &'a Option<Vec<String>>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            let mut files = Vec::new();
            let mut read_dir = fs::read_dir(dir).await?;

            let exclusion_regex = regex::Regex::new(
                r"node_modules|\.git|\.svn|\.hg|coverage|dist|build|out|\.next|\.nuxt",
            )?;

            while let Some(entry) = read_dir.next_entry().await? {
                let path = entry.path();

                if path.is_dir() {
                    if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                        if !exclusion_regex.is_match(dir_name) {
                            let sub_files = Self::scan_directory(&path, filter_extensions).await?;
                            files.extend(sub_files);
                        }
                    }
                } else if path.is_file() && Self::should_include_file(&path, filter_extensions) {
                    files.push(path.to_string_lossy().to_string());
                }
            }

            Ok(files)
        })
    }

    /// Determine if a file should be included based on extension filters
    pub fn should_include_file(path: &Path, filter_extensions: &Option<Vec<String>>) -> bool {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| format!(".{}", ext))
            .unwrap_or_default();

        match filter_extensions {
            Some(filters) => filters.iter().any(|filter_ext| extension == *filter_ext),
            None => {
                matches!(
                    extension.as_str(),
                    ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".json" | ".vue"
                )
            }
        }
    }

    pub fn get_watch_directories(files: &[String]) -> Vec<String> {
        let mut watched_dirs = std::collections::HashSet::new();

        for file in files {
            if let Some(parent) = Path::new(file).parent() {
                watched_dirs.insert(parent.to_string_lossy().to_string());
            }
        }

        watched_dirs.into_iter().collect()
    }
}

pub mod path_utils {
    use std::path::Path;

    pub fn normalize_path_string(path: &str) -> String {
        Path::new(path).to_string_lossy().replace("\\", "/")
    }

    pub fn is_supported_file_type(path: &Path) -> bool {
        if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
            matches!(
                extension,
                "js" | "jsx" | "ts" | "tsx" | "mjs" | "json" | "vue"
            )
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn test_normalize_path_string() {
        assert_eq!(
            path_utils::normalize_path_string("src\\main.rs"),
            "src/main.rs"
        );

        assert_eq!(
            path_utils::normalize_path_string("src/main.rs"),
            "src/main.rs"
        );
    }
}
