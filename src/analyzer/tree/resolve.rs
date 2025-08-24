use crate::parser::ModuleResolver;
use crate::types::DependencyTree;
use crate::types::ParseOptions;
use anyhow::Result;
use std::path::Path;

pub async fn resolve_dependencies(
    resolver: &ModuleResolver,
    tree: &mut DependencyTree,
    options: &ParseOptions,
) -> Result<()> {
    let mut all_resolutions = Vec::new();

    for (file_id, deps_opt) in tree.iter() {
        if let Some(dependencies) = deps_opt {
            let context = Path::new(file_id).parent().unwrap_or(Path::new("."));

            for dep in dependencies {
                if let Ok(Some(resolved)) = resolver
                    .resolve_module(context, &dep.request, &options.extensions)
                    .await
                {
                    let normalized = crate::utils::path::normalize_path_for_storage(&resolved)?;
                    all_resolutions.push((file_id.clone(), dep.request.clone(), normalized));
                }
            }
        }
    }

    for (file_id, request, resolved_id) in all_resolutions {
        if let Some(Some(dependencies)) = tree.get_mut(&file_id) {
            for dep in dependencies {
                if dep.request == request {
                    dep.id = Some(resolved_id.clone());
                    break;
                }
            }
        }
    }

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
