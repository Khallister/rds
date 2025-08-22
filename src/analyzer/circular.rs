use crate::types::{DependencyTree, DependencyKind, SkipDynamicImports};
use std::collections::{HashMap, HashSet};

pub struct CircularAnalyzer;

impl CircularAnalyzer {
    pub fn new() -> Self {
        Self
    }
    
    pub fn find_circular_dependencies(
        &self,
        tree: &DependencyTree,
        skip_dynamic_imports: &SkipDynamicImports,
        max_count: Option<usize>,
    ) -> Vec<Vec<String>> {
        let mut circulars = Vec::new();
        let mut tree_copy = tree.clone();
        
        // Visit all nodes to find cycles
        for id in tree.keys() {
            // Early exit if we've found enough circular dependencies
            if let Some(max) = max_count {
                if circulars.len() >= max {
                    break;
                }
            }
            
            if tree_copy.contains_key(id) {
                self.visit_node(
                    id.clone(), 
                    Vec::new(), 
                    &mut tree_copy, 
                    &mut circulars, 
                    skip_dynamic_imports,
                    max_count
                );
            }
        }
        
        circulars
    }
    
    fn visit_node(
        &self,
        id: String,
        mut path: Vec<String>,
        tree: &mut DependencyTree,
        circulars: &mut Vec<Vec<String>>,
        skip_dynamic_imports: &SkipDynamicImports,
        max_count: Option<usize>,
    ) {
        // Early exit if we've found enough circular dependencies
        if let Some(max) = max_count {
            if circulars.len() >= max {
                return;
            }
        }
        
        // Check if we've found a cycle
        if let Some(index) = path.iter().position(|x| x == &id) {
            // Found a cycle - extract the circular part
            let circular = path[index..].to_vec();
            circulars.push(circular);
            return;
        }
        
        // If this node was already processed, skip
        if !tree.contains_key(&id) {
            return;
        }
        
        // Get dependencies and remove from tree to avoid reprocessing
        let deps = tree.remove(&id);
        if let Some(Some(dependencies)) = deps {
            path.push(id.clone());
            
            for dep in dependencies {
                // Early exit if we've found enough circular dependencies
                if let Some(max) = max_count {
                    if circulars.len() >= max {
                        break;
                    }
                }
                
                if let Some(dep_id) = &dep.id {
                    // Skip dynamic imports if configured to do so for circular detection
                    if *skip_dynamic_imports == SkipDynamicImports::Circular 
                        && dep.kind == DependencyKind::DynamicImport {
                        continue;
                    }
                    
                    self.visit_node(dep_id.clone(), path.clone(), tree, circulars, skip_dynamic_imports, max_count);
                }
            }
        }
    }
}
