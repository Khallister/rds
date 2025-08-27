use crate::utils;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::sync::RwLock;

pub struct ModuleResolver {
    builtin_modules: std::collections::HashSet<String>,
    // simple in-memory cache: key -> resolved path (or None)
    cache: RwLock<HashMap<String, Option<String>>>,
    // cache file contents for repeated package.json / tsconfig reads
    file_cache: RwLock<HashMap<String, Option<String>>>,
    // cache parsed JSON files like package.json and tsconfig.json
    parsed_json_cache: RwLock<HashMap<String, Option<serde_json::Value>>>,
    // cache path metadata checks to avoid repeated fs::metadata calls during a run
    path_kind_cache: RwLock<HashMap<String, u8>>,
    // cache directory listings: dir path -> (timestamp_millis, Vec<(entry_name, kind)>)
    dir_listing_cache: RwLock<HashMap<String, (u128, Vec<(String, u8)>)>>,
    // cache canonicalize results (path -> canonicalized normalized string)
    canonicalize_cache: RwLock<HashMap<String, String>>,
}

#[cfg(test)]
mod tests;

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

        Self {
            builtin_modules,
            cache: RwLock::new(HashMap::new()),
            file_cache: RwLock::new(HashMap::new()),
            parsed_json_cache: RwLock::new(HashMap::new()),
            path_kind_cache: RwLock::new(HashMap::new()),
            dir_listing_cache: RwLock::new(HashMap::new()),
            canonicalize_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Clear all in-memory resolver caches (used by watch-mode on large changes)
    pub async fn clear_all_caches(&self) {
        let mut c = self.cache.write().await;
        c.clear();
        let mut f = self.file_cache.write().await;
        f.clear();
        let mut p = self.path_kind_cache.write().await;
        p.clear();
        let mut d = self.dir_listing_cache.write().await;
        d.clear();
        let mut pj = self.parsed_json_cache.write().await;
        pj.clear();
        let mut can = self.canonicalize_cache.write().await;
        can.clear();
    }

    /// Invalidate cache entries related to the provided paths. This performs
    /// targeted removals: file content cache, path-kind entries, parent dir
    /// listings and resolver entries that resolved to the given paths.
    pub async fn invalidate_paths(&self, paths: &[String]) {
        use std::path::Path;

        let mut cache_write = self.cache.write().await;
        let mut file_cache_write = self.file_cache.write().await;
        let mut kind_write = self.path_kind_cache.write().await;
        let mut dir_write = self.dir_listing_cache.write().await;

        for p in paths {
            // Normalize the incoming path for comparison
            let np = Self::normalize_path(p);

            // Remove parsed file content entries (both raw and normalized keys)
            file_cache_write.remove(&np);
            file_cache_write.remove(p);

            // Remove parsed JSON entries
            let mut pj_write = self.parsed_json_cache.write().await;
            pj_write.remove(&np);
            pj_write.remove(p);

            // Remove canonicalize cache entries
            let mut can_write = self.canonicalize_cache.write().await;
            can_write.remove(&np);
            can_write.remove(p);

            // Remove path-kind entries
            kind_write.remove(&np);
            kind_write.remove(p);

            // Remove resolver memoization entries that pointed to this path
            let keys_to_remove: Vec<String> = cache_write
                .iter()
                .filter_map(|(k, v)| {
                    if let Some(ref val) = v {
                        if val == &np || val == p {
                            Some(k.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            for k in keys_to_remove {
                cache_write.remove(&k);
            }

            // Remove parent directory listing caches as a change inside the
            // directory may affect resolution candidates (new files, deleted files)
            if let Some(parent) = Path::new(p).parent() {
                let key = parent.to_string_lossy().to_string();
                dir_write.remove(&key);
                let key2 = Self::normalize_pathbuf(parent);
                dir_write.remove(&key2);
            }
        }
    }

    async fn path_kind_cached(&self, path: &str) -> u8 {
        // 0 = missing, 1 = file, 2 = dir
        {
            let read = self.path_kind_cache.read().await;
            if let Some(v) = read.get(path) {
                return *v;
            }
        }

        let kind = match fs::metadata(path).await {
            Ok(meta) => {
                if meta.is_file() {
                    1u8
                } else if meta.is_dir() {
                    2u8
                } else {
                    0u8
                }
            }
            Err(_) => 0u8,
        };

        let mut write = self.path_kind_cache.write().await;
        write.insert(path.to_string(), kind);
        kind
    }

    async fn read_dir_listing_cached(&self, dir: &Path) -> Option<Vec<(String, u8)>> {
        let key = dir.to_string_lossy().to_string();
        {
            let read = self.dir_listing_cache.read().await;
            if let Some((ts, v)) = read.get(&key) {
                // TTL: 5 seconds
                use std::time::{SystemTime, UNIX_EPOCH};
                let now_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let age_ms = now_ms.saturating_sub(*ts);
                if *ts != 0 && age_ms <= 5000 {
                    return Some(v.clone());
                }
                // otherwise fallthrough to refresh
            }
        }

        let mut entries: Vec<(String, u8)> = Vec::new();
        let rd = fs::read_dir(dir).await;
        if rd.is_err() {
            let mut write = self.dir_listing_cache.write().await;
            write.insert(key.clone(), (0u128, Vec::new()));
            return None;
        }

        let mut stream = rd.unwrap();
        while let Ok(Some(ent)) = stream.next_entry().await {
            let file_name = ent.file_name().to_string_lossy().to_string();
            let kind = match ent.metadata().await {
                Ok(m) => {
                    if m.is_file() {
                        1u8
                    } else if m.is_dir() {
                        2u8
                    } else {
                        0u8
                    }
                }
                Err(_) => 0u8,
            };
            entries.push((file_name, kind));
        }

        use std::time::{SystemTime, UNIX_EPOCH};
        let mut write = self.dir_listing_cache.write().await;
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        write.insert(key.clone(), (now_ms, entries.clone()));
        if entries.is_empty() {
            None
        } else {
            Some(entries)
        }
    }

    async fn read_file_cached<P: AsRef<Path>>(&self, path: P) -> Option<String> {
        let key = path.as_ref().to_string_lossy().to_string();
        // check cache
        {
            let read = self.file_cache.read().await;
            if let Some(cached) = read.get(&key) {
                return cached.clone();
            }
        }

        // not cached: try reading
        match utils::read_file_text_async(path.as_ref()).await {
            Ok(content) => {
                let mut write = self.file_cache.write().await;
                write.insert(key.clone(), Some(content.clone()));
                Some(content)
            }
            Err(_) => {
                let mut write = self.file_cache.write().await;
                write.insert(key.clone(), None);
                None
            }
        }
    }

    /// Read and parse JSON files (package.json, tsconfig.json) and cache
    /// the parsed serde_json::Value. Returns None if file missing or parse fails.
    async fn read_parsed_json_cached<P: AsRef<Path>>(&self, path: P) -> Option<serde_json::Value> {
        let key = path.as_ref().to_string_lossy().to_string();
        {
            let read = self.parsed_json_cache.read().await;
            if let Some(v) = read.get(&key) {
                return v.clone();
            }
        }

        if let Some(content) = self.read_file_cached(&path).await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                let mut write = self.parsed_json_cache.write().await;
                write.insert(key.clone(), Some(parsed.clone()));
                return Some(parsed);
            }
        }

        let mut write = self.parsed_json_cache.write().await;
        write.insert(key.clone(), None);
        None
    }

    /// Canonicalize a path (blocking call) but cache the result to avoid repeated
    /// canonicalize calls. Falls back to lexical normalization when canonicalize fails.
    async fn canonicalize_cached(&self, path: &str) -> String {
        {
            let read = self.canonicalize_cache.read().await;
            if let Some(v) = read.get(path) {
                return v.clone();
            }
        }

        // perform blocking canonicalize in spawn_blocking
        let path_string = path.to_string();
        let res = tokio::task::spawn_blocking(move || std::fs::canonicalize(&path_string)).await;
        let normalized = match res {
            Ok(Ok(pbuf)) => {
                // Convert to string and strip Windows long-path prefix if present
                let mut s = pbuf.to_string_lossy().to_string();
                if cfg!(windows) {
                    if s.starts_with(r"\\?\") {
                        s = s[4..].to_string();
                    }
                }
                Self::normalize_path(&s)
            }
            _ => Self::normalize_path(path),
        };

        let mut write = self.canonicalize_cache.write().await;
        write.insert(path.to_string(), normalized.clone());
        normalized
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
        let start = std::time::Instant::now();
        let context = context.as_ref();

        // memoization key: "<context>|<request>"
        let ctx_s = context.to_string_lossy();
        let key = format!("{}|{}", ctx_s, request);

        // check cache first
        {
            let read = self.cache.read().await;
            if let Some(cached) = read.get(&key) {
                return Ok(cached.clone());
            }
        }

        if self.builtin_modules.contains(request) {
            let res = Some(request.to_string());
            let mut write = self.cache.write().await;
            write.insert(key, res.clone());
            return Ok(res);
        }

        if let Some(resolved) = self.resolve_ts_alias(context, request, extensions).await? {
            let res = Some(resolved);
            // cache positive resolution only
            let mut write = self.cache.write().await;
            write.insert(key, res.clone());
            return Ok(res);
        }

        if Path::new(request).is_absolute() {
            let out = self.append_suffix(request, extensions).await?;
            // cache only positive results
            if out.is_some() {
                let mut write = self.cache.write().await;
                write.insert(key, out.clone());
            }
            return Ok(out);
        }

        if request.starts_with('.') {
            let resolved = context.join(request);
            let normalized_path = self.canonicalize_cached(&resolved.to_string_lossy()).await;
            let out = self.append_suffix(&normalized_path, extensions).await?;
            if out.is_some() {
                let mut write = self.cache.write().await;
                write.insert(key, out.clone());
            }
            return Ok(out);
        }

        let result = self
            .resolve_node_module(context, request, extensions)
            .await?;
        let out = if let Some(path) = result {
            Some(Self::normalize_path(&path))
        } else {
            None
        };

        if out.is_some() {
            let mut write = self.cache.write().await;
            write.insert(key, out.clone());
        }
        if std::env::var("RDS_WATCH_DEBUG").is_ok() {
            let elapsed = start.elapsed();
            eprintln!(
                "[resolver] resolve_module: context='{}' request='{}' elapsed={}ms",
                context.display(),
                request,
                elapsed.as_millis()
            );
        }
        Ok(out)
    }

    fn append_suffix<'a>(
        &'a self,
        request: &'a str,
        extensions: &'a [String],
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>>
    {
        Box::pin(async move {
            // FIX: Potential improvement by not doing repeated fs reads here, but rather store the metadata in helper variable?
            // It could also work out to get the list of request's directory contents, filter them out by exact match/extensions/directories
            // And then do the path normalization, instead of repeated fs reads
            // Try to use directory listing to avoid many metadata calls.
            if let Some(parent) = Path::new(request).parent() {
                if let Some(listing) = self.read_dir_listing_cached(parent).await {
                    let base = Path::new(request)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| request.to_string());

                    // exact match
                    for (name, kind) in &listing {
                        if name == &base && *kind == 1u8 {
                            return Ok(Some(Self::normalize_path(request)));
                        }
                    }

                    // extension candidates
                    for ext in extensions {
                        let candidate = format!("{}{}", base, ext);
                        for (name, kind) in &listing {
                            if name == &candidate && *kind == 1u8 {
                                let path_with_ext = format!("{}{}", request, ext);
                                return Ok(Some(Self::normalize_path(&path_with_ext)));
                            }
                        }
                    }
                }
            }

            // Fallback to single metadata checks (cached) when listing isn't available
            if self.path_kind_cached(request).await == 1 {
                return Ok(Some(Self::normalize_path(request)));
            }

            for ext in extensions {
                let path_with_ext = format!("{}{}", request, ext);
                if self.path_kind_cached(&path_with_ext).await == 1 {
                    return Ok(Some(Self::normalize_path(&path_with_ext)));
                }
            }

            if self.path_kind_cached(request).await == 2 {
                let index_path = Path::new(request).join("index");
                let normalized_index = self
                    .canonicalize_cached(&index_path.to_string_lossy())
                    .await;
                return Box::pin(self.append_suffix(&normalized_index, extensions)).await;
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
            if let Some(package_json) = self.read_parsed_json_cached(&package_json_path).await {
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

            // Try append_suffix on the node_modules candidate; read_file_cached is file-only
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

            if let Some(tsconfig) = self.read_parsed_json_cached(&tsconfig_path).await {
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

                                let normalized_resolved = self
                                    .canonicalize_cached(&resolved_path.to_string_lossy())
                                    .await;

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

                            let normalized_target = self
                                .canonicalize_cached(&target_path.to_string_lossy())
                                .await;

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
