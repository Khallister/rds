#[cfg(test)]
use crate::types::Dependency;
use crate::types::DependencyTree;
use std::collections::{HashMap, HashSet, VecDeque};

/// ReverseIndex maintains a map from a resolved id to a set of issuer file keys
/// that import that id, encapsulating insertion, removal, and pruning logic so the
/// tree builder doesn't have to manage the raw HashMap directly.
///
/// Additionally, it maintains a `base_name_map`, which maps file base names to all
/// keys (full resolved ids) with that base name. This is used for fast lookup of
/// dependencies when only the file name is known or when matching by suffix is required,
/// such as during affected set computation or when handling ambiguous or partial paths.
#[derive(Debug, Default, Clone)]
pub struct ReverseIndex {
    idx: HashMap<String, HashSet<String>>,
    /// Maps file base names to all keys with that base name for fast lookup,
    /// enabling efficient reverse lookups and suffix-based matching.
    base_name_map: HashMap<String, HashSet<String>>,
}

impl ReverseIndex {
    pub fn new() -> Self {
        Self {
            idx: HashMap::new(),
            base_name_map: HashMap::new(),
        }
    }

    pub fn from_tree(tree: &DependencyTree) -> Self {
        let mut ri = ReverseIndex::new();
        for (k, deps_opt) in tree.iter() {
            if let Some(deps) = deps_opt {
                for dep in deps {
                    if let Some(ref id) = dep.id {
                        ri.idx
                            .entry(id.clone())
                            .or_insert_with(HashSet::new)
                            .insert(k.clone());
                        // Update base_name_map
                        if let Some(base) = std::path::Path::new(id)
                            .file_name()
                            .and_then(|s| s.to_str())
                        {
                            ri.base_name_map
                                .entry(base.to_string())
                                .or_insert_with(HashSet::new)
                                .insert(id.clone());
                        }
                    }
                }
            }
        }
        ri
    }

    /// Update mappings for a single issuer key by removing entries derived
    /// from `prev_deps_opt` and inserting those from `new_deps_opt`.
    // This method is only used in tests and is not part of the public API in production.
    #[cfg(test)]
    pub fn update_mappings_for_issuer(
        &mut self,
        issuer: &str,
        prev_deps_opt: Option<&Vec<Dependency>>,
        new_deps_opt: Option<&Vec<Dependency>>,
    ) {
        if let Some(prev_deps) = prev_deps_opt {
            for dep in prev_deps {
                if let Some(ref id) = dep.id {
                    if let Some(set) = self.idx.get_mut(id) {
                        set.remove(issuer);
                    }
                }
            }
        }

        // Clean up any empty sets left behind
        let ids_to_remove: Vec<String> = self
            .idx
            .iter()
            .filter_map(|(id, set)| {
                if set.is_empty() {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        for id in ids_to_remove {
            self.idx.remove(&id);
        }

        if let Some(new_deps) = new_deps_opt {
            for dep in new_deps {
                if let Some(ref id) = dep.id {
                    self.idx
                        .entry(id.clone())
                        .or_insert_with(HashSet::new)
                        .insert(issuer.to_string());
                }
            }
        }
    }

    /// Remove parent references that do not exist in `last_full_tree` anymore.
    pub fn prune(&mut self, last_full_tree: &DependencyTree) {
        let last_keys: HashSet<String> = last_full_tree.keys().cloned().collect();

        let mut ids_to_remove: Vec<String> = Vec::new();

        for (id, parents) in self.idx.iter_mut() {
            parents.retain(|p| last_keys.contains(p));
            if parents.is_empty() {
                ids_to_remove.push(id.clone());
            }
        }

        for id in ids_to_remove {
            self.idx.remove(&id);
        }
    }

    pub fn get_parents(&self, id: &str) -> Option<Vec<String>> {
        self.idx.get(id).map(|s| s.iter().cloned().collect())
    }

    pub fn keys(&self) -> Vec<String> {
        self.idx.keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.idx.is_empty()
    }

    /// Return a mutable reference to the parent set for an id, if present.
    /// This method is only used in tests and is not part of the public API in production.
    #[cfg(test)]
    pub fn get_mut_parents(&mut self, id: &str) -> Option<&mut HashSet<String>> {
        self.idx.get_mut(id)
    }

    /// Remove an id entry entirely from the index.
    /// This method is only used in tests and is not part of the public API in production.
    #[cfg(test)]
    pub fn remove_id(&mut self, id: &str) {
        self.idx.remove(id);
    }

    /// Merge a partial tree into the provided last_full_tree and update the
    /// reverse index accordingly. This will replace entries for keys present
    /// in `partial` inside `last_full_tree` and update index mappings.
    pub fn merge_partial_into_full(
        &mut self,
        partial: &DependencyTree,
        last_full_tree: &mut DependencyTree,
    ) {
        let rds_watch_debug = std::env::var("RDS_WATCH_DEBUG").is_ok();

        for (k, v) in partial.iter() {
            // remove previous mappings for this issuer
            if let Some(prev_opt) = last_full_tree.get(k) {
                if let Some(prev_deps) = prev_opt {
                    for dep in prev_deps {
                        if let Some(ref id) = dep.id {
                            if let Some(set) = self.idx.get_mut(id) {
                                let removed = set.remove(k);
                                if rds_watch_debug && removed {
                                    crate::logger::info(&format!(
                                        "[ReverseIndex] removed mapping: id={} issuer={}",
                                        id, k
                                    ));
                                }
                                if set.is_empty() {
                                    // will be cleaned later
                                }
                            }
                        }
                    }
                }
            }
            // replace entry in last_full_tree
            last_full_tree.insert(k.clone(), v.clone());

            // insert new mappings
            if let Some(deps) = v {
                for dep in deps {
                    if let Some(ref id) = dep.id {
                        let inserted = self
                            .idx
                            .entry(id.clone())
                            .or_insert_with(HashSet::new)
                            .insert(k.clone());
                        if rds_watch_debug && inserted {
                            crate::logger::info(&format!(
                                "[ReverseIndex] inserted mapping: id={} issuer={}",
                                id, k
                            ));
                        }
                    }
                }
            }
        }

        // cleanup any empty sets
        let ids_to_remove: Vec<String> = self
            .idx
            .iter()
            .filter_map(|(id, set)| {
                if set.is_empty() {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        for id in ids_to_remove {
            self.idx.remove(&id);
        }
    }
    /// Computes the set of affected files given a list of changed files.
    ///
    /// This function is async because it performs asynchronous path normalization
    /// for each changed file before traversing the reverse index.
    pub async fn compute_affected_set(&self, changed_files: &[String]) -> HashSet<String> {
        if self.idx.is_empty() {
            return HashSet::new();
        }

        let rds_watch_debug = std::env::var("RDS_WATCH_DEBUG").is_ok();

        let mut queue: VecDeque<String> = VecDeque::new();
        let affected = HashSet::new();

        // Batch normalization of changed_files
        let futures = changed_files
            .iter()
            .map(|f| crate::utils::path::normalize_path_for_storage_cached(f));
        let normalized_results = futures::future::join_all(futures).await;

        let mut seen: HashSet<String> = HashSet::new();

        for (_, result) in normalized_results.into_iter().enumerate() {
            if let Ok(nf) = result {
                if seen.insert(nf.clone()) {
                    queue.push_back(nf.clone());
                }

                if self.idx.contains_key(&nf) {
                    continue;
                }

                if let Some(base) = std::path::Path::new(&nf)
                    .file_name()
                    .and_then(|s| s.to_str())
                {
                    // Use base_name_map for fast lookup
                    if let Some(keys) = self.base_name_map.get(base) {
                        for key in keys {
                            if seen.insert(key.clone()) {
                                queue.push_back(key.clone());
                            }
                        }
                        continue;
                    }

                    // Fallback to suffix matching if not found in base_name_map
                    for key in self.idx.keys() {
                        if key.ends_with(&nf) || nf.ends_with(key) || key.ends_with(base) {
                            if seen.insert(key.clone()) {
                                queue.push_back(key.clone());
                            }
                        }
                    }
                } else {
                    for key in self.idx.keys() {
                        if key.ends_with(&nf) || nf.ends_with(key) {
                            if seen.insert(key.clone()) {
                                queue.push_back(key.clone());
                            }
                        }
                    }
                }
            }
        }

        if rds_watch_debug {
            crate::logger::info(&format!(
                "[ReverseIndex::compute_affected_set] changed_files={:?}, initial_queue={:?}, index_keys_count={} ",
                changed_files,
                queue,
                self.idx.len()
            ));
        }

        self.bfs_traverse(queue, affected)
    }

    /// Helper for BFS traversal used in compute_affected_set and potentially elsewhere.
    fn bfs_traverse(
        &self,
        mut queue: VecDeque<String>,
        mut affected: HashSet<String>,
    ) -> HashSet<String> {
        while let Some(curr) = queue.pop_front() {
            if affected.contains(&curr) {
                continue;
            }
            affected.insert(curr.clone());

            if let Some(parents) = self.idx.get(&curr) {
                for p in parents.iter() {
                    if !affected.contains(p) {
                        queue.push_back(p.clone());
                    }
                }
            }
        }
        affected
    }
}
