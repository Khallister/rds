use crate::types::Dependency;
use crate::types::DependencyTree;
use std::collections::{HashMap, HashSet};

/// ReverseIndex maintains a map from a resolved id -> set of issuer file keys
/// that import that id. It encapsulates insertion/removal/pruning logic so the
/// tree builder doesn't have to manage the raw HashMap directly.
#[derive(Debug, Default, Clone)]
pub struct ReverseIndex {
    idx: HashMap<String, HashSet<String>>,
}

impl ReverseIndex {
    pub fn new() -> Self {
        Self {
            idx: HashMap::new(),
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
                    }
                }
            }
        }
        ri
    }

    /// Update mappings for a single issuer key by removing entries derived
    /// from `prev_deps_opt` and inserting those from `new_deps_opt`.
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
                        if set.is_empty() {
                            // Mark for removal by dropping the empty set
                        }
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
    pub fn get_mut_parents(&mut self, id: &str) -> Option<&mut HashSet<String>> {
        self.idx.get_mut(id)
    }

    /// Remove an id entry entirely from the index.
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
        for (k, v) in partial.iter() {
            // remove previous mappings for this issuer
            if let Some(prev_opt) = last_full_tree.get(k) {
                if let Some(prev_deps) = prev_opt {
                    for dep in prev_deps {
                        if let Some(ref id) = dep.id {
                            if let Some(set) = self.idx.get_mut(id) {
                                let removed = set.remove(k);
                                if std::env::var("RDS_WATCH_DEBUG").is_ok() && removed {
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
                        if std::env::var("RDS_WATCH_DEBUG").is_ok() && inserted {
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

    /// Compute affected set using the internal reverse index only. Returns an
    /// empty set if the index is empty.
    pub async fn compute_affected_set(&self, changed_files: &[String]) -> HashSet<String> {
        if self.idx.is_empty() {
            return HashSet::new();
        }

        let mut queue: Vec<String> = Vec::new();
        let mut affected: HashSet<String> = HashSet::new();

        for f in changed_files {
            if let Some(nf) = crate::utils::path::normalize_path_for_storage_cached(f)
                .await
                .ok()
            {
                // Always include the changed file itself in the affected set so
                // callers see the direct change even when no parents exist.
                queue.push(nf.clone());

                // Direct match (absolute or already-shortened)
                if self.idx.contains_key(&nf) {
                    continue;
                }

                // Base filename (e.g., "file.js") may match keys in shortened trees
                if let Some(base) = std::path::Path::new(&nf)
                    .file_name()
                    .and_then(|s| s.to_str())
                {
                    if self.idx.contains_key(base) {
                        queue.push(base.to_string());
                        continue;
                    }

                    // Fallback: try suffix-match against stored keys so absolute
                    // normalized paths can match shortened keys like "src/foo.js".
                    for key in self.idx.keys() {
                        if key.ends_with(&nf) || nf.ends_with(key) || key.ends_with(base) {
                            queue.push(key.clone());
                        }
                    }
                } else {
                    // If no base, still attempt suffix matches
                    for key in self.idx.keys() {
                        if key.ends_with(&nf) || nf.ends_with(key) {
                            queue.push(key.clone());
                        }
                    }
                }
            }
        }

        // Optional debug output to help trace why certain changed files map (or
        // don't map) to index keys during watch-mode incremental runs.
        if std::env::var("RDS_WATCH_DEBUG").is_ok() {
            crate::logger::info(&format!(
                "[ReverseIndex::compute_affected_set] changed_files={:?}, initial_queue={:?}, index_keys_count={} ",
                changed_files,
                queue,
                self.idx.len()
            ));
        }

        while let Some(curr) = queue.pop() {
            if affected.contains(&curr) {
                continue;
            }
            affected.insert(curr.clone());

            if let Some(parents) = self.idx.get(&curr) {
                for p in parents.iter() {
                    if !affected.contains(p) {
                        queue.push(p.clone());
                    }
                }
            }
        }

        affected
    }
}
