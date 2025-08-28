use crate::types::{DependencyKind, DependencyTree, SkipDynamicImports};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

pub struct CircularAnalyzer;

impl CircularAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Find circular dependencies using a single DFS per node with coloring
    /// to avoid revisiting nodes. This runs in O(N + E) time and avoids
    /// cloning the tree repeatedly.
    ///
    /// # Returns
    ///
    /// Returns a vector of cycles, where each cycle is represented as a vector of module IDs (`Vec<String>`).
    /// Each inner vector contains the sequence of module IDs that form a circular dependency.
    pub fn find_circular_dependencies(
        &self,
        tree: &DependencyTree,
        skip_dynamic_imports: &SkipDynamicImports,
        max_count: Option<usize>,
    ) -> Vec<Vec<String>> {
        let t0 = Instant::now();
        crate::logger::info(&format!(
            "find_circular_dependencies: starting (tree entries={})",
            tree.len()
        ));

        let mut circulars: Vec<Vec<String>> = Vec::new();
        let mut seen_cycles: HashSet<String> = HashSet::new();

        // color: 0 = unvisited, 1 = visiting (on stack), 2 = done
        let mut color: HashMap<String, u8> = HashMap::new();
        let mut stack: Vec<String> = Vec::new();
        let mut stack_indices: HashMap<String, usize> = HashMap::new();

        // helper to canonicalize and insert cycle if new
        fn canonicalize_and_insert(
            cyc: Vec<String>,
            seen_cycles: &mut HashSet<String>,
            circulars: &mut Vec<Vec<String>>,
        ) {
            if cyc.is_empty() {
                return;
            }

            let forward = rotations(&cyc[..]);
            let mut backward_vec = cyc.clone();
            backward_vec.reverse();
            let backward = rotations(&backward_vec[..]);

            if let (Some(forward), Some(backward)) = (forward, backward) {
                let forward_key = forward.join("->");
                let backward_key = backward.join("->");
                let canonical = if forward_key <= backward_key {
                    forward
                } else {
                    backward
                };

                let key = canonical.join("->");
                if !seen_cycles.contains(&key) {
                    seen_cycles.insert(key);
                    circulars.push(canonical);
                }
            }
        }

        // Extracted helper function for rotations
        fn rotations(v: &[String]) -> Option<Vec<String>> {
            let len = v.len();
            if len == 0 {
                return None;
            }
            let mut best = None::<(String, Vec<String>)>;
            for i in 0..len {
                let mut r = v.to_vec();
                r.rotate_left(i);
                let key = r.join("->");
                match &best {
                    Some((k, _)) if k > &key => {}
                    _ => best = Some((key, r)),
                }
            }
            best.map(|(_, v)| v)
        }

        // recursive DFS function (using & to avoid captures)
        fn dfs(
            node: &str,
            tree: &DependencyTree,
            skip_dynamic_imports: &SkipDynamicImports,
            max_count: Option<usize>,
            color: &mut HashMap<String, u8>,
            stack: &mut Vec<String>,
            stack_indices: &mut HashMap<String, usize>,
            circulars: &mut Vec<Vec<String>>,
            seen_cycles: &mut HashSet<String>,
        ) {
            if let Some(max) = max_count {
                if circulars.len() >= max {
                    return;
                }
            }

            color.insert(node.to_string(), 1);
            stack_indices.insert(node.to_string(), stack.len());
            stack.push(node.to_string());

            if let Some(Some(deps)) = tree.get(node) {
                for dep in deps {
                    if let Some(max) = max_count {
                        if circulars.len() >= max {
                            break;
                        }
                    }

                    if let Some(dep_id) = &dep.id {
                        if *skip_dynamic_imports == SkipDynamicImports::Circular
                            && dep.kind == DependencyKind::DynamicImport
                        {
                            continue;
                        }

                        let state = color.get(dep_id).copied().unwrap_or(0);
                        if state == 1 {
                            // back-edge -> found a cycle
                            if let Some(&pos) = stack_indices.get(dep_id) {
                                let cyc: Vec<&str> =
                                    stack[pos..].iter().map(|s| s.as_str()).collect();
                                // Only clone when passing to canonicalize_and_insert
                                canonicalize_and_insert(
                                    cyc.iter().map(|s| s.to_string()).collect(),
                                    seen_cycles,
                                    circulars,
                                );
                            }
                        } else if state == 0 {
                            dfs(
                                dep_id,
                                tree,
                                skip_dynamic_imports,
                                max_count,
                                color,
                                stack,
                                stack_indices,
                                circulars,
                                seen_cycles,
                            );
                        }
                    }
                }
            }

            stack.pop();
            stack_indices.remove(node);
            color.insert(node.to_string(), 2);
        }

        for id in tree.keys() {
            if let Some(max) = max_count {
                if circulars.len() >= max {
                    break;
                }
            }

            if color.get(id).copied().unwrap_or(0) == 0 {
                dfs(
                    id,
                    tree,
                    skip_dynamic_imports,
                    max_count,
                    &mut color,
                    &mut stack,
                    &mut stack_indices,
                    &mut circulars,
                    &mut seen_cycles,
                );
            }
        }

        let elapsed = t0.elapsed();
        crate::logger::info(&format!(
            "find_circular_dependencies: completed (found={} circulars) elapsed={}ms",
            circulars.len(),
            elapsed.as_millis()
        ));

        circulars
    }
}
