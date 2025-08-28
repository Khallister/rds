use crate::parser::ModuleResolver;
use crate::types::DependencyTree;
use crate::types::ParseOptions;
use anyhow::Result;
use futures::stream::{self, StreamExt};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

pub async fn resolve_dependencies(
    resolver: &ModuleResolver,
    tree: &mut DependencyTree,
    options: &ParseOptions,
) -> Result<()> {
    crate::logger::info(&format!(
        "resolve_dependencies: starting (tree entries={})",
        tree.len()
    ));
    let t0 = Instant::now();
    // Collect all unresolved dependency requests into a task list so we can
    // resolve them in parallel.
    let mut tasks: Vec<(String, String, std::path::PathBuf)> = Vec::new();
    for (file_id, deps_opt) in tree.iter() {
        if let Some(dependencies) = deps_opt {
            let context = Path::new(file_id).parent().unwrap_or(Path::new("."));

            for dep in dependencies {
                if dep.id.is_none() {
                    tasks.push((file_id.clone(), dep.request.clone(), context.to_path_buf()));
                }
            }
        }
    }

    let concurrency = if let Some(cfg) = options.resolve_concurrency {
        // enforce reasonable caps
        let cap = 256usize;
        std::cmp::min(cfg, cap)
    } else {
        rayon::current_num_threads().min(64)
    };
    let exts = Arc::new(options.extensions.clone());

    let results = stream::iter(tasks)
        .map(|(file_id, request, context)| {
            let resolver = resolver;
            let exts = exts.clone();
            async move {
                let res = resolver.resolve_module(&context, &request, &*exts).await;
                (file_id, request, res)
            }
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await;

    let mut all_resolutions: Vec<(String, String, String)> = Vec::new();
    let mut resolved_count = 0usize;
    for (file_id, request, res) in results {
        if let Ok(Some(resolved)) = res {
            let normalized =
                crate::utils::path::normalize_path_for_storage_cached(&resolved).await?;
            all_resolutions.push((file_id, request, normalized));
            resolved_count += 1;
        }
    }

    crate::logger::info(&format!(
        "resolve_dependencies: resolved {}/{} requests",
        resolved_count,
        all_resolutions.len()
    ));

    for (file_id, request, resolved_id) in &all_resolutions {
        if let Some(Some(dependencies)) = tree.get_mut(file_id) {
            for dep in dependencies {
                if dep.request == *request {
                    dep.id = Some(resolved_id.clone());
                    break;
                }
            }
        }
    }

    let elapsed = t0.elapsed();
    crate::logger::info(&format!(
        "resolve_dependencies: completed (resolved={} entries) elapsed={:?}",
        all_resolutions.len(),
        elapsed
    ));

    Ok(())
}

pub fn shorten_tree(context: &Path, tree: DependencyTree) -> Result<DependencyTree> {
    let mut shortened = DependencyTree::new();

    for (key, deps_opt) in tree {
        let short_key = Path::new(&key)
            .strip_prefix(context)
            .unwrap_or(Path::new(&key))
            .to_string_lossy()
            .replace('\\', "/");

        let shortened_deps = if let Some(dependencies) = deps_opt {
            Some(
                dependencies
                    .into_iter()
                    .map(|mut dep| {
                        dep.issuer = short_key.clone();
                        if let Some(ref id) = dep.id {
                            let normalized_id = Path::new(id)
                                .strip_prefix(context)
                                .unwrap_or(Path::new(id))
                                .to_string_lossy()
                                .replace('\\', "/");
                            dep.id = Some(normalized_id);
                        }
                        dep
                    })
                    .collect(),
            )
        } else {
            None
        };

        shortened.insert(short_key, shortened_deps);
    }

    Ok(shortened)
}
