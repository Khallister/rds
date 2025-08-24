use crate::types::{Dependency, DependencyKind};
use anyhow::Result;
use regex::Regex;
use std::path::Path;

pub struct JavaScriptParser {
    import_regex: Regex,
    require_regex: Regex,
    dynamic_import_regex: Regex,
    export_from_regex: Regex,
}

impl JavaScriptParser {
    pub fn new() -> Result<Self> {
        Ok(Self {
            import_regex: Regex::new(r#"import\s+(?:[^'"\n]+\s+from\s+)?['"]([^'\"]+)['"]"#)?,
            require_regex: Regex::new(r#"require\s*\(\s*['\"]([^'\"]+)['\"]\s*\)"#)?,
            dynamic_import_regex: Regex::new(r#"import\s*\(\s*['\"]([^'\"]+)['\"]\s*\)"#)?,
            export_from_regex: Regex::new(
                r#"export\s+(?:[^'"\n]+\s+)?from\s+['\"]([^'\"]+)['\"]"#,
            )?,
        })
    }

    pub fn handled_extensions(&self) -> Vec<String> {
        vec![
            "js".to_string(),
            "mjs".to_string(),
            "cjs".to_string(),
            "jsx".to_string(),
            "ts".to_string(),
            "tsx".to_string(),
        ]
    }

    pub fn parse_file<P: AsRef<Path>>(
        &self,
        file_path: P,
        content: &str,
    ) -> Result<Vec<Dependency>> {
        let file_path = file_path.as_ref();
        let issuer = file_path.to_string_lossy().to_string();
        let mut dependencies = Vec::new();
        let cleaned_content = self.remove_comments(content);

        for caps in self.import_regex.captures_iter(&cleaned_content) {
            if let Some(module) = caps.get(1) {
                dependencies.push(Dependency {
                    issuer: issuer.clone(),
                    request: module.as_str().to_string(),
                    kind: DependencyKind::StaticImport,
                    id: None,
                });
            }
        }

        for caps in self.require_regex.captures_iter(&cleaned_content) {
            if let Some(module) = caps.get(1) {
                dependencies.push(Dependency {
                    issuer: issuer.clone(),
                    request: module.as_str().to_string(),
                    kind: DependencyKind::CommonJS,
                    id: None,
                });
            }
        }

        for caps in self.dynamic_import_regex.captures_iter(&cleaned_content) {
            if let Some(module) = caps.get(1) {
                dependencies.push(Dependency {
                    issuer: issuer.clone(),
                    request: module.as_str().to_string(),
                    kind: DependencyKind::DynamicImport,
                    id: None,
                });
            }
        }

        for caps in self.export_from_regex.captures_iter(&cleaned_content) {
            if let Some(module) = caps.get(1) {
                dependencies.push(Dependency {
                    issuer: issuer.clone(),
                    request: module.as_str().to_string(),
                    kind: DependencyKind::StaticExport,
                    id: None,
                });
            }
        }

        Ok(dependencies)
    }

    fn remove_comments(&self, content: &str) -> String {
        let mut result = String::new();
        let mut chars = content.chars().peekable();
        let mut in_single_comment = false;
        let mut in_multi_comment = false;

        while let Some(ch) = chars.next() {
            if in_single_comment {
                if ch == '\n' {
                    in_single_comment = false;
                    result.push(ch);
                }
                continue;
            }

            if in_multi_comment {
                if ch == '*' {
                    if let Some(&'/') = chars.peek() {
                        chars.next();
                        in_multi_comment = false;
                    }
                }
                continue;
            }

            match ch {
                '"' | '\'' | '`' => {
                    let delim = ch;
                    // Keep the opening delimiter so checks of preceding content are simple
                    // (we examine `result` to see what precedes this string)
                    // We will decide whether this string is a module specifier (part of
                    // an import/require/export/from) and avoid scrubbing in that case.
                    let mut inner = String::new();
                    while let Some(next_ch) = chars.next() {
                        if next_ch == '\\' {
                            inner.push(next_ch);
                            if let Some(escaped) = chars.next() {
                                inner.push(escaped);
                                continue;
                            } else {
                                break;
                            }
                        }
                        if next_ch == delim {
                            // decide if this string looks like a module specifier by
                            // inspecting the tail of `result` (what immediately precedes
                            // the opening quote). If it looks like `from `, `import `,
                            // `import(` or `require(` (with optional space), we assume
                            // it's a module specifier and keep the inner text unchanged.
                            let lookback_len = 50usize;
                            let tail_chars: String =
                                result.chars().rev().take(lookback_len).collect();
                            let tail: String = tail_chars.chars().rev().collect();

                            // normalize some spacing for simple suffix checks
                            let tail_norm = tail.replace("\t", " ");

                            let is_module_specifier = tail_norm.ends_with("from ")
                                || tail_norm.ends_with("import ")
                                || tail_norm.ends_with("import(")
                                || tail_norm.ends_with("import (")
                                || tail_norm.ends_with("require(")
                                || tail_norm.ends_with("require (");

                            if is_module_specifier {
                                result.push(delim);
                                result.push_str(&inner);
                                result.push(delim);
                            } else {
                                // scrub import-like keywords inside the string so regexes
                                // don't detect keywords that appear in plain strings
                                let scrubbed = inner
                                    .replace("import", "__IMPORT__")
                                    .replace("require", "__REQUIRE__")
                                    .replace("export", "__EXPORT__")
                                    .replace("from", "__FROM__");
                                result.push(delim);
                                result.push_str(&scrubbed);
                                result.push(delim);
                            }

                            break;
                        }
                        inner.push(next_ch);
                    }
                }
                '/' => {
                    if let Some(&next_ch) = chars.peek() {
                        match next_ch {
                            '/' => {
                                chars.next();
                                in_single_comment = true;
                            }
                            '*' => {
                                chars.next();
                                in_multi_comment = true;
                            }
                            _ => result.push(ch),
                        }
                    } else {
                        result.push(ch);
                    }
                }
                _ => result.push(ch),
            }
        }

        result
    }
}
