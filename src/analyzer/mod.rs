pub mod circular;
pub mod tree;
pub mod unused;

use crate::cache::CacheStats;
use crate::types::{AnalysisResult, DependencyTree, ParseOptions};
use anyhow::Result;
use circular::CircularAnalyzer;
use tree::TreeBuilder;

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

    pub async fn analyze_files(&mut self, entries: &[String]) -> Result<(AnalysisResult, usize)> {
        let (tree, num_threads) = self
            .tree_builder
            .build_dependency_tree(entries, &self.options)
            .await?;

        let circulars = self.circular_analyzer.find_circular_dependencies(
            &tree,
            &self.options.skip_dynamic_imports,
            self.options.take,
        );

        let resolved_entries = entries.to_vec();

        let result = AnalysisResult {
            entries: resolved_entries,
            tree,
            circulars,
        };

        Ok((result, num_threads))
    }

    pub async fn analyze_files_incremental(
        &mut self,
        changed_files: &[String],
    ) -> Result<(AnalysisResult, usize)> {
        let (tree, num_threads) = self
            .tree_builder
            .build_dependency_tree_incremental(changed_files, &self.options)
            .await?;

        let circulars = self.circular_analyzer.find_circular_dependencies(
            &tree,
            &self.options.skip_dynamic_imports,
            self.options.take,
        );

        let resolved_entries = changed_files.to_vec();

        let result = AnalysisResult {
            entries: resolved_entries,
            tree,
            circulars,
        };

        Ok((result, num_threads))
    }

    pub fn get_cache_stats(&self) -> CacheStats {
        self.tree_builder.get_cache_stats()
    }

    pub fn get_incremental_cache_stats(&mut self) -> CacheStats {
        self.tree_builder.get_incremental_cache_stats()
    }

    pub fn analyze_warnings(&self, tree: &DependencyTree) -> Vec<String> {
        let mut warnings = Vec::new();

        for (file_id, deps_opt) in tree {
            if deps_opt.is_none() {
                warnings.push(format!("skip {:?}, excluded or not found", file_id));
            } else if let Some(dependencies) = deps_opt {
                for dep in dependencies {
                    if dep.id.is_none() {
                        warnings.push(format!("miss {:?} in {:?}", dep.request, dep.issuer));
                    }
                }
            }
        }

        warnings.sort();
        warnings
    }

    pub async fn detect_unused_files(
        &self,
        pattern: &str,
        tree: &DependencyTree,
    ) -> Result<Vec<String>> {
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
