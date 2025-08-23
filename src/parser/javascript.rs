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
            import_regex: Regex::new(r#"import\s+(?:[^"']+\s+from\s+)?["']([^"']+)["']"#)?,
            require_regex: Regex::new(r#"require\s*\(\s*["']([^"']+)["']\s*\)"#)?,
            dynamic_import_regex: Regex::new(r#"import\s*\(\s*["']([^"']+)["']\s*\)"#)?,
            export_from_regex: Regex::new(r#"export\s+(?:[^"']+\s+)?from\s+["']([^"']+)["']"#)?,
        })
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
        let mut in_string = false;
        let mut string_delim = '"';
        let mut in_single_comment = false;
        let mut in_multi_comment = false;

        while let Some(ch) = chars.next() {
            match ch {
                '"' | '\'' if !in_single_comment && !in_multi_comment => {
                    if !in_string {
                        in_string = true;
                        string_delim = ch;
                    } else if ch == string_delim {
                        in_string = false;
                    }
                    result.push(ch);
                }
                '\\' if in_string => {
                    result.push(ch);
                    if let Some(next_ch) = chars.next() {
                        result.push(next_ch);
                    }
                }
                '/' if !in_string && !in_single_comment && !in_multi_comment => {
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
                '\n' if in_single_comment => {
                    in_single_comment = false;
                    result.push(ch);
                }
                '*' if in_multi_comment => {
                    if let Some(&'/') = chars.peek() {
                        chars.next();
                        in_multi_comment = false;
                    }
                }
                _ => {
                    if !in_single_comment && !in_multi_comment {
                        result.push(ch);
                    }
                }
            }
        }

        result
    }
}
