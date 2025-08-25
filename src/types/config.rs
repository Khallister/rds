use regex::Regex;
use std::path::PathBuf;

pub struct ParseOptions {
    pub context: PathBuf,
    pub extensions: Vec<String>,
    pub include: Regex,
    pub exclude: Regex,
    pub dependency_exclude: Regex,
    pub tsconfig: Option<PathBuf>,
    pub skip_dynamic_imports: SkipDynamicImports,
    pub progress_callback: Option<Box<dyn Fn(ProgressEvent, &str) + Send + Sync>>,
    pub take: Option<usize>,
    pub cache_enabled: bool,
}

impl std::fmt::Debug for ParseOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParseOptions")
            .field("context", &self.context)
            .field("extensions", &self.extensions)
            .field("include", &self.include.as_str())
            .field("exclude", &self.exclude.as_str())
            .field("dependency_exclude", &self.dependency_exclude.as_str())
            .field("tsconfig", &self.tsconfig)
            .field("skip_dynamic_imports", &self.skip_dynamic_imports)
            .field("progress_callback", &"<function>")
            .field("take", &self.take)
            .field("cache_enabled", &self.cache_enabled)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkipDynamicImports {
    Never,
    Tree,
    Circular,
}

#[derive(Debug, Clone)]
pub enum ProgressEvent {
    Start,
    End,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            context: std::env::current_dir().unwrap_or_default(),
            extensions: vec![
                "".to_string(),
                ".ts".to_string(),
                ".tsx".to_string(),
                ".mjs".to_string(),
                ".js".to_string(),
                ".jsx".to_string(),
                ".json".to_string(),
                ".vue".to_string(),
            ],

            include: Regex::new(".*").unwrap(),
            exclude: Regex::new(
                r"node_modules|\.git|\.svn|\.hg|coverage|dist|build|out|\.next|\.nuxt",
            )
            .unwrap(),
            dependency_exclude: Regex::new(r"node_modules|\.git|\.svn|\.hg").unwrap(),
            tsconfig: None,

            skip_dynamic_imports: SkipDynamicImports::Never,
            progress_callback: None,
            take: None,
            cache_enabled: true,
        }
    }
}
