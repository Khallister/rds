use super::javascript::JavaScriptParser;
use crate::types::{Dependency, DependencyKind};
use anyhow::{Context, Result};
use regex::Regex;
use std::path::Path;

pub struct VueParser {
    js_parser: JavaScriptParser,
    component_regex: Regex,
    import_regex: Regex,
    script_regex: Regex,
    script_setup_regex: Regex,
    template_regex: Regex,
    style_regex: Regex,
}

impl VueParser {
    pub fn new() -> Result<Self> {
        Ok(Self {
            js_parser: JavaScriptParser::new()?,
            component_regex: Regex::new(r"<(\w+)")?,
            import_regex: Regex::new(r#"@import\s+['"]([^'"]+)['"]"#)?,
            script_regex: Regex::new(r"(?s)<script(?:\s[^>]*)?>(.*?)</script>")?,
            script_setup_regex: Regex::new(r"(?s)<script[^>]*setup[^>]*>(.*?)</script>")?,
            template_regex: Regex::new(r"(?s)<template(?:\s[^>]*)?>(.*?)</template>")?,
            style_regex: Regex::new(r"(?s)<style(?:\s[^>]*)?>(.*?)</style>")?,
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

        let sfc = self.parse_sfc(content)?;

        if let Some(script) = &sfc.script {
            let script_deps = self
                .js_parser
                .parse_file(file_path, &script.content)
                .context("Failed to parse Vue script section")?;

            dependencies.extend(script_deps.into_iter().map(|mut dep| {
                dep.kind = DependencyKind::VueScript;
                dep
            }));
        }

        if let Some(script_setup) = &sfc.script_setup {
            let setup_deps = self
                .js_parser
                .parse_file(file_path, &script_setup.content)
                .context("Failed to parse Vue setup script section")?;

            dependencies.extend(setup_deps.into_iter().map(|mut dep| {
                dep.kind = DependencyKind::VueScript;
                dep
            }));
        }

        if let Some(template) = &sfc.template {
            let template_deps = self.parse_vue_template(&template.content, &issuer)?;
            dependencies.extend(template_deps);
        }

        for style in &sfc.styles {
            let style_deps = self.parse_vue_style(&style.content, &issuer)?;
            dependencies.extend(style_deps);
        }

        Ok(dependencies)
    }

    fn parse_sfc(&self, content: &str) -> Result<SfcDescriptor> {
        let script = self
            .script_regex
            .captures(content)
            .filter(|cap| {
                let full_tag = &content[cap.get(0).unwrap().range()];
                !full_tag.contains("setup")
            })
            .map(|cap| SfcBlock {
                content: cap.get(1).unwrap().as_str().to_string(),
            });

        let script_setup = self
            .script_setup_regex
            .captures(content)
            .map(|cap| SfcBlock {
                content: cap.get(1).unwrap().as_str().to_string(),
            });

        let template = self.template_regex.captures(content).map(|cap| SfcBlock {
            content: cap.get(1).unwrap().as_str().to_string(),
        });

        let styles = self
            .style_regex
            .captures_iter(content)
            .map(|cap| SfcBlock {
                content: cap.get(1).unwrap().as_str().to_string(),
            })
            .collect();

        Ok(SfcDescriptor {
            script,
            script_setup,
            template,
            styles,
        })
    }

    fn parse_vue_template(&self, content: &str, issuer: &str) -> Result<Vec<Dependency>> {
        let mut dependencies = Vec::new();

        for cap in self.component_regex.captures_iter(content) {
            if let Some(component_name) = cap.get(1) {
                let name = component_name.as_str();
                if name.chars().next().unwrap_or('a').is_uppercase() {
                    dependencies.push(Dependency {
                        issuer: issuer.to_string(),
                        request: format!("@/components/{}.vue", name),
                        kind: DependencyKind::VueTemplate,
                        id: None,
                    });
                }
            }
        }

        Ok(dependencies)
    }

    fn parse_vue_style(&self, content: &str, issuer: &str) -> Result<Vec<Dependency>> {
        let mut dependencies = Vec::new();

        for cap in self.import_regex.captures_iter(content) {
            if let Some(import_path) = cap.get(1) {
                dependencies.push(Dependency {
                    issuer: issuer.to_string(),
                    request: import_path.as_str().to_string(),
                    kind: DependencyKind::VueStyle,
                    id: None,
                });
            }
        }

        Ok(dependencies)
    }
}

#[derive(Debug)]
struct SfcDescriptor {
    script: Option<SfcBlock>,
    script_setup: Option<SfcBlock>,
    template: Option<SfcBlock>,
    styles: Vec<SfcBlock>,
}

#[derive(Debug)]
struct SfcBlock {
    content: String,
}
