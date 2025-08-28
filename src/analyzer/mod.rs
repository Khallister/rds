pub mod circular;
pub mod tree;

use crate::cache::CacheStats;
use crate::types::{AnalysisResult, ParseOptions};
use anyhow::Result;
use circular::CircularAnalyzer;
use tree::TreeBuilder;

#[cfg(test)]
mod tests;

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

    /// Invalidate caches related to the provided paths. Async because
    /// resolver operations are async.
    pub async fn invalidate_caches(&mut self, paths: &[String]) {
        self.tree_builder.invalidate_caches(paths).await;
    }

    /// Clear all analyzer-related caches.
    pub async fn clear_all_caches(&mut self) {
        self.tree_builder.clear_all_caches().await;
    }

    pub fn options(&self) -> &ParseOptions {
        &self.options
    }
}
