pub mod plugins;
pub mod resolver;

pub use plugins::JavaScriptParser;
pub use plugins::VueParser;
pub use resolver::ModuleResolver;

use anyhow::Result;
use once_cell::sync::Lazy;
use std::sync::{Arc, RwLock};

/// Parser trait: implementors can parse a file content and return discovered dependencies.
/// Object-safe: takes file path as &str to allow dynamic dispatch.
pub trait Parser: Send + Sync {
    fn parse_file(&self, file_path: &str, content: &str) -> Result<Vec<crate::types::Dependency>>;
    /// Return the list of file extensions (without dot) this parser handles, e.g. ["js", "mjs"].
    fn handled_extensions(&self) -> Vec<String>;
}

/// Delegate existing parsers to the trait so they can be used as dyn Parser.
impl Parser for JavaScriptParser {
    fn parse_file(&self, file_path: &str, content: &str) -> Result<Vec<crate::types::Dependency>> {
        JavaScriptParser::parse_file(self, file_path, content)
    }
    fn handled_extensions(&self) -> Vec<String> {
        JavaScriptParser::handled_extensions(self)
    }
}

impl Parser for VueParser {
    fn parse_file(&self, file_path: &str, content: &str) -> Result<Vec<crate::types::Dependency>> {
        VueParser::parse_file(self, file_path, content)
    }
    fn handled_extensions(&self) -> Vec<String> {
        VueParser::handled_extensions(self)
    }
}

pub type DynParser = Arc<dyn Parser>;

static PARSER_REGISTRY: Lazy<RwLock<Vec<(Vec<String>, DynParser)>>> =
    Lazy::new(|| RwLock::new(Vec::new()));

pub struct ParserFactory;

impl ParserFactory {
    pub fn get_parser_for_extension(ext: &str) -> Result<Option<DynParser>> {
        if let Some(p) = get_registered_parser_for_extension(ext) {
            return Ok(Some(p));
        }

        match ext.to_lowercase().as_str() {
            "vue" => Ok(Some(Arc::new(VueParser::new()?))),
            "js" | "cjs" | "mjs" | "jsx" | "ts" | "tsx" => {
                Ok(Some(Arc::new(JavaScriptParser::new()?)))
            }
            _ => Ok(None),
        }
    }

    /// Register a parser for given extensions (e.g., ["vue"]) at runtime.
    pub fn register_parser_for_extensions(exts: Vec<&str>, parser: DynParser) {
        let mut reg = PARSER_REGISTRY.write().unwrap();
        let exts_owned = exts.into_iter().map(|s| s.to_string()).collect();
        reg.push((exts_owned, parser));
    }
}

/// Free function wrapper to register parser (keeps API ergonomic for plugins/tests).
pub fn register_parser_for_extensions(exts: Vec<&str>, parser: DynParser) {
    ParserFactory::register_parser_for_extensions(exts, parser);
}

/// Register a parser by asking it which extensions it handles and delegating
/// to the per-extension registration API.
pub fn register_parser(parser: DynParser) {
    let exts = parser.handled_extensions();
    let refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
    register_parser_for_extensions(refs, parser);
}

/// Try to find a registered parser by extension.
pub fn get_registered_parser_for_extension(ext: &str) -> Option<DynParser> {
    let reg = PARSER_REGISTRY.read().unwrap();
    for (exts, parser) in reg.iter() {
        if exts.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
            return Some(parser.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests;
