pub mod tree;
pub mod circular;
pub mod unused;

use anyhow::Result;
use crate::types::{AnalysisResult, DependencyTree, ParseOptions};
use tree::TreeBuilder;
use circular::CircularAnalyzer;

pub struct DependencyAnalyzer {
    tree_builder: TreeBuilder,
    circular_analyzer: CircularAnalyzer,
    options: ParseOptions,
}

impl DependencyAnalyzer {
    pub fn new(options: ParseOptions) -> Result<Self> {
        Ok(Self {
            tree_builder: TreeBuilder::new()?,
            circular_analyzer: CircularAnalyzer::new(),
            options,
        })
    }
    
    pub async fn analyze_files(&self, entries: &[String]) -> Result<AnalysisResult> {
        // Build dependency tree
        let tree = self.tree_builder.build_dependency_tree(entries, &self.options).await?;
        
        // Find circular dependencies
        let circulars = self.circular_analyzer.find_circular_dependencies(&tree, &self.options.skip_dynamic_imports, self.options.take);
        
        // Convert entries to resolved paths (simplified for now)
        let resolved_entries = entries.to_vec();
        
        Ok(AnalysisResult {
            entries: resolved_entries,
            tree,
            circulars,
        })
    }
    
    pub fn analyze_warnings(&self, tree: &DependencyTree) -> Vec<String> {
        let mut warnings = Vec::new();
        
        for (file_id, deps_opt) in tree {
            if deps_opt.is_none() {
                warnings.push(format!("skip {:?}, excluded or not found", file_id));
            } else if let Some(dependencies) = deps_opt {
                for dep in dependencies {
                    if dep.id.is_none() {
                        warnings.push(format!(
                            "miss {:?} in {:?}",
                            dep.request, dep.issuer
                        ));
                    }
                }
            }
        }
        
        warnings.sort();
        warnings
    }
    
    pub async fn detect_unused_files(&self, pattern: &str, tree: &DependencyTree) -> Result<Vec<String>> {
        let all_files: Vec<_> = glob::glob(pattern)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        
        let used_files: std::collections::HashSet<_> = tree.keys().collect();
        
        let unused: Vec<_> = all_files
            .into_iter()
            .filter(|f| !used_files.contains(f))
            .collect();
        
        Ok(unused)
    }
}
