use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyKind {
    #[serde(rename = "CommonJS")]
    CommonJS,
    #[serde(rename = "StaticImport")]
    StaticImport,
    #[serde(rename = "DynamicImport")]
    DynamicImport,
    #[serde(rename = "StaticExport")]
    StaticExport,
    #[serde(rename = "VueTemplate")]
    VueTemplate,
    #[serde(rename = "VueScript")]
    VueScript,
    #[serde(rename = "VueStyle")]
    VueStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub issuer: String,
    pub request: String,
    pub kind: DependencyKind,
    pub id: Option<String>,
}

pub type DependencyTree = HashMap<String, Option<Vec<Dependency>>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub entries: Vec<String>,
    pub tree: DependencyTree,
    pub circulars: Vec<Vec<String>>,
}
