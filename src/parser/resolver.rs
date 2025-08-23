use crate::utils;
use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::fs;

pub struct ModuleResolver {
    builtin_modules: std::collections::HashSet<String>,
}

impl ModuleResolver {
    pub fn new() -> Self {
        let mut builtin_modules = std::collections::HashSet::new();
        for module in &[
            "assert",
            "buffer",
            "child_process",
            "cluster",
            "crypto",
            "dgram",
            "dns",
            "domain",
            "events",
            "fs",
            "http",
            "https",
            "module",
            "net",
            "os",
            "path",
            "punycode",
            "querystring",
            "readline",
            "stream",
            "string_decoder",
            "timers",
            "tls",
            "tty",
            "url",
            "util",
            "vm",
            "zlib",
        ] {
            builtin_modules.insert(module.to_string());
        }

        Self { builtin_modules }
    }

    fn normalize_path(path: &str) -> String {
        let normalized = if cfg!(windows) {
            path.replace('/', "\\")
        } else {
            path.replace('\\', "/")
        };

        let separator = if cfg!(windows) { "\\" } else { "/" };
        let parts: Vec<&str> = normalized
            .split(separator)
            .filter(|p| !p.is_empty())
            .collect();
        let mut result = Vec::new();

        for part in parts {
            match part {
                ".." => {
                    if !result.is_empty() && result.last() != Some(&"..") {
                        result.pop();
                    } else {
                        result.push(part);
                    }
                }
                "." => {}
                "" => {}
                _ => {
                    result.push(part);
                }
            }
        }

        let result_path = result.join(separator);
        if normalized.starts_with(separator) {
            format!("{}{}", separator, result_path)
        } else {
            result_path
        }
    }

    fn normalize_pathbuf(path: &Path) -> String {
        Self::normalize_path(&path.to_string_lossy())
    }

    pub async fn resolve_module<P: AsRef<Path>>(
        &self,
        context: P,
        request: &str,
        extensions: &[String],
    ) -> Result<Option<String>> {
        let context = context.as_ref();

        if self.builtin_modules.contains(request) {
            return Ok(Some(request.to_string()));
        }

        if let Some(resolved) = self.resolve_ts_alias(context, request, extensions).await? {
            return Ok(Some(resolved));
        }

        if Path::new(request).is_absolute() {
            return self.append_suffix(request, extensions).await;
        }

        if request.starts_with('.') {
            let resolved = context.join(request);
            let normalized_path = Self::normalize_pathbuf(&resolved);
            return self.append_suffix(&normalized_path, extensions).await;
        }

        let result = self
            .resolve_node_module(context, request, extensions)
            .await?;
        if let Some(path) = result {
            Ok(Some(Self::normalize_path(&path)))
        } else {
            Ok(None)
        }
    }

    fn append_suffix<'a>(
        &'a self,
        request: &'a str,
        extensions: &'a [String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>>
    {
        Box::pin(async move {
            if let Ok(metadata) = fs::metadata(request).await {
                if metadata.is_file() {
                    return Ok(Some(Self::normalize_path(request)));
                }
            }

            for ext in extensions {
                let path_with_ext = format!("{}{}", request, ext);
                if let Ok(metadata) = fs::metadata(&path_with_ext).await {
                    if metadata.is_file() {
                        return Ok(Some(Self::normalize_path(&path_with_ext)));
                    }
                }
            }

            if let Ok(metadata) = fs::metadata(request).await {
                if metadata.is_dir() {
                    let index_path = Path::new(request).join("index");
                    let normalized_index = Self::normalize_pathbuf(&index_path);
                    return Box::pin(self.append_suffix(&normalized_index, extensions)).await;
                }
            }

            Ok(None)
        })
    }

    async fn resolve_node_module<P: AsRef<Path>>(
        &self,
        context: P,
        request: &str,
        extensions: &[String],
    ) -> Result<Option<String>> {
        let mut current = context.as_ref().to_path_buf();

        loop {
            let node_modules = current.join("node_modules").join(request);

            let package_json_path = node_modules.join("package.json");
            if let Ok(package_content) = utils::read_file_text_async(&package_json_path).await {
                if let Ok(package_json) =
                    serde_json::from_str::<serde_json::Value>(&package_content)
                {
                    let main_field = package_json
                        .get("module")
                        .or_else(|| package_json.get("main"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("index.js");

                    let main_path = node_modules.join(main_field);
                    if let Some(resolved) = self
                        .append_suffix(&main_path.to_string_lossy(), extensions)
                        .await?
                    {
                        return Ok(Some(resolved));
                    }
                }
            }

            if let Some(resolved) = self
                .append_suffix(&node_modules.to_string_lossy(), extensions)
                .await?
            {
                return Ok(Some(resolved));
            }

            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => break,
            }
        }

        Ok(None)
    }

    async fn resolve_ts_alias(
        &self,
        context: &Path,
        request: &str,
        extensions: &[String],
    ) -> Result<Option<String>> {
        if request.starts_with('@')
            && !request.starts_with("@/")
            && request.chars().skip(1).any(|c| c == '/')
        {
            let parts: Vec<&str> = request.splitn(3, '/').collect();
            if parts.len() >= 2 && parts[0].starts_with('@') && !parts[1].is_empty() {
                return Ok(None);
            }
        }

        let mut current_dir = context.to_path_buf();

        loop {
            let tsconfig_path = current_dir.join("tsconfig.json");

            if let Ok(content) = utils::read_file_text_async(&tsconfig_path).await {
                if let Ok(tsconfig) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(paths) = tsconfig
                        .get("compilerOptions")
                        .and_then(|c| c.get("paths"))
                        .and_then(|p| p.as_object())
                    {
                        for (pattern, targets) in paths {
                            if let Some(resolved) = self
                                .match_ts_path_pattern(
                                    &tsconfig_path.parent().unwrap(),
                                    request,
                                    pattern,
                                    targets,
                                    extensions,
                                )
                                .await?
                            {
                                return Ok(Some(resolved));
                            }
                        }
                    }
                }
            }

            match current_dir.parent() {
                Some(parent) => current_dir = parent.to_path_buf(),
                None => break,
            }
        }

        Ok(None)
    }

    async fn match_ts_path_pattern(
        &self,
        base_dir: &Path,
        request: &str,
        pattern: &str,
        targets: &serde_json::Value,
        extensions: &[String],
    ) -> Result<Option<String>> {
        if pattern.ends_with("/*") {
            let pattern_prefix = &pattern[..pattern.len() - 2];

            if request.starts_with(pattern_prefix) {
                let remaining = &request[pattern_prefix.len()..];

                if let Some(target_array) = targets.as_array() {
                    for target in target_array {
                        if let Some(target_str) = target.as_str() {
                            if target_str.ends_with("/*") {
                                let target_base = &target_str[..target_str.len() - 2];

                                let target_path = if target_base.starts_with("./") {
                                    base_dir.join(&target_base[2..])
                                } else if target_base.starts_with("/") {
                                    PathBuf::from(target_base)
                                } else {
                                    base_dir.join(target_base)
                                };

                                let resolved_path = if remaining.is_empty() || remaining == "/" {
                                    target_path
                                } else if remaining.starts_with('/') {
                                    target_path.join(&remaining[1..])
                                } else {
                                    target_path.join(remaining)
                                };

                                let normalized_resolved = Self::normalize_pathbuf(&resolved_path);

                                if let Some(result) =
                                    self.append_suffix(&normalized_resolved, extensions).await?
                                {
                                    return Ok(Some(result));
                                }
                            }
                        }
                    }
                }
            }
        } else {
            if request == pattern {
                if let Some(target_array) = targets.as_array() {
                    for target in target_array {
                        if let Some(target_str) = target.as_str() {
                            let target_path = if target_str.starts_with("./") {
                                base_dir.join(&target_str[2..])
                            } else if target_str.starts_with("/") {
                                PathBuf::from(target_str)
                            } else {
                                base_dir.join(target_str)
                            };

                            let normalized_target = Self::normalize_pathbuf(&target_path);

                            if let Some(result) =
                                self.append_suffix(&normalized_target, extensions).await?
                            {
                                return Ok(Some(result));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }
}
