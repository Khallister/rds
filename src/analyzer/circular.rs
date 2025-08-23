use crate::types::{DependencyTree, DependencyKind, SkipDynamicImports};
use std::collections::HashSet;

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
        let tree_copy = tree.clone();

        // Use a set to deduplicate cycles that are the same but rotated
        let mut seen_cycles: HashSet<String> = HashSet::new();

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
                    &tree_copy,
                    &mut circulars,
                    skip_dynamic_imports,
                    max_count,
                    &mut seen_cycles,
                );
            }
        }

        circulars
    }
    
    fn visit_node(
        &self,
        id: String,
        mut path: Vec<String>,
        tree: &DependencyTree,
        circulars: &mut Vec<Vec<String>>,
        skip_dynamic_imports: &SkipDynamicImports,
        max_count: Option<usize>,
        seen_cycles: &mut HashSet<String>,
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
            let mut circular = path[index..].to_vec();

            // Canonicalize the cycle by choosing the lexicographically smallest
            // rotation among the cycle and its reverse. This ensures that
            // rotated or reversed duplicates map to the same canonical form.
            fn canonicalize(mut cyc: Vec<String>) -> Vec<String> {
                if cyc.is_empty() {
                    return cyc;
                }

                // Generate all rotations for a given vector and return the minimal by join
                let rotations = |v: &Vec<String>| {
                    let len = v.len();
                    let mut best = None::<(String, Vec<String>)>;
                    for i in 0..len {
                        let mut r = v.clone();
                        r.rotate_left(i);
                        let key = r.join("->");
                        match &best {
                            Some((k, _)) if k <= &key => {}
                            _ => best = Some((key, r)),
                        }
                    }
                    best.map(|(_, v)| v).unwrap()
                };

                let forward = rotations(&cyc);
                cyc.reverse();
                let backward = rotations(&cyc);

                // Choose lexicographically smallest of forward/backward
                if forward.join("->") <= backward.join("->") {
                    forward
                } else {
                    backward
                }
            }

            let canonical = canonicalize(circular);
            let key = canonical.join("->");
            if !seen_cycles.contains(&key) {
                seen_cycles.insert(key);
                circulars.push(canonical);
            }

            return;
        }
        
        // If this node is not present, skip
        if !tree.contains_key(&id) {
            return;
        }

        // Get dependencies (clone so we don't mutate original tree)
        if let Some(Some(dependencies)) = tree.get(&id).cloned() {
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
                    
                    self.visit_node(
                        dep_id.clone(),
                        path.clone(),
                        tree,
                        circulars,
                        skip_dynamic_imports,
                        max_count,
                        seen_cycles,
                    );
                }
            }
        }
    }
}
