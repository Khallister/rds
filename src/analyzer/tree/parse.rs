use crate::cache::FileCache;
use crate::parser::ModuleResolver;
use crate::types::{Dependency, DependencyTree, ParseOptions};
use anyhow::{Error, Result};
use futures::stream::StreamExt;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
// ...existing code...
pub async fn parse_files_batch<'a>(
    files: Vec<String>,
    options: &'a ParseOptions,
    max_concurrent: usize,
) -> Vec<Result<(String, Option<Vec<Dependency>>), (String, Error)>> {
    let mut file_results = futures::stream::iter(files)
        .map(|file_path| {
            let file_path_clone = file_path.clone();
            let opts = options;
            Box::pin(async move {
                match parse_file_static(&file_path_clone, opts).await {
                    Ok(v) => Ok(v),
                    Err(e) => Err((file_path_clone.clone(), e)),
                }
            })
        })
        .buffer_unordered(max_concurrent);

    let mut results = Vec::new();
    while let Some(r) = file_results.next().await {
        results.push(r);
    }
    results
}

pub async fn parse_file_static(
    file_path: &str,
    options: &ParseOptions,
) -> Result<(String, Option<Vec<Dependency>>)> {
    if !options.include.is_match(file_path) || options.exclude.is_match(file_path) {
        return Ok((file_path.to_string(), None));
    }

    let path = Path::new(file_path);
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Resolve parser dynamically by extension. If no parser (runtime-registered or built-in)
    // is available for this extension, treat the file as non-parseable (empty deps).
    let parser_opt = crate::parser::ParserFactory::get_parser_for_extension(extension)?;
    if parser_opt.is_none() {
        return Ok((file_path.to_string(), Some(Vec::new())));
    }
    let parser = parser_opt.unwrap();

    if let Some(ref callback) = options.progress_callback {
        callback(crate::types::ProgressEvent::Start, file_path);
    }

    let read_path_buf = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else if file_path.starts_with("../../") || file_path.starts_with("..\\..\\") {
        let cwd = std::env::current_dir().unwrap_or_default();
        let rest = &file_path[6..];
        let parent = cwd.parent().unwrap_or(&cwd);
        parent.join(rest)
    } else {
        options.context.join(file_path)
    };

    let content = crate::utils::read_file_text_async(&read_path_buf).await?;

    let dependencies = parser.parse_file(file_path, &content)?;

    if let Some(ref callback) = options.progress_callback {
        callback(crate::types::ProgressEvent::End, file_path);
    }

    Ok((file_path.to_string(), Some(dependencies)))
}

pub async fn process_parsed_results(
    cache: &mut FileCache,
    resolver: &ModuleResolver,
    parsed_results: Vec<Result<(String, Option<Vec<Dependency>>), (String, Error)>>,
    tree: &mut DependencyTree,
    processed_files: &mut HashSet<String>,
    new_dependencies: &mut Vec<String>,
    options: &ParseOptions,
) -> Result<()> {
    for result in parsed_results {
        match result {
            Ok((file_path, dependencies_opt)) => {
                if let Some(mut deps) = dependencies_opt {
                    let cache_key = crate::utils::path::normalize_path_for_storage(&file_path)?;

                    // Resolve dependency requests to absolute/normalized ids when possible
                    let context = Path::new(&file_path).parent().unwrap_or(Path::new("."));
                    for dep in deps.iter_mut() {
                        if dep.id.is_none() {
                            if let Ok(Some(resolved_path)) = resolver
                                .resolve_module(context, &dep.request, &options.extensions)
                                .await
                            {
                                if let Ok(norm) =
                                    crate::utils::path::normalize_path_for_storage(&resolved_path)
                                {
                                    dep.id = Some(norm);
                                }
                            }
                        }
                    }

                    // Cache enriched dependencies (with ids when resolved)
                    cache
                        .cache_dependencies(&file_path, &cache_key, deps.clone())
                        .await?;

                    let normalized_path =
                        crate::utils::path::normalize_path_for_storage(&file_path)?;
                    tree.insert(normalized_path.clone(), Some(deps.clone()));

                    // Use resolved ids from deps when available to avoid re-resolving
                    for dep in deps {
                        if let Some(resolved_id) = dep.id {
                            if !processed_files.contains(&resolved_id)
                                && !new_dependencies.contains(&resolved_id)
                            {
                                new_dependencies.push(resolved_id);
                            }
                        } else if let Ok(Some(resolved_path)) = resolver
                            .resolve_module(context, &dep.request, &options.extensions)
                            .await
                        {
                            let normalized =
                                crate::utils::path::normalize_path_for_storage(&resolved_path)?;
                            if !processed_files.contains(&normalized)
                                && !new_dependencies.contains(&normalized)
                            {
                                new_dependencies.push(normalized);
                            }
                        }
                    }
                } else {
                    let normalized_path =
                        crate::utils::path::normalize_path_for_storage(&file_path)?;
                    tree.insert(normalized_path, None);
                }
            }
            Err((file_path, error)) => {
                return Err(anyhow::anyhow!(
                    "Failed to parse file {}: {}",
                    file_path,
                    error
                ));
            }
        }
    }
    Ok(())
}

pub async fn parse_single_file_deps(
    _cache: &mut FileCache,
    file_path: &str,
    options: &ParseOptions,
    tree: &mut DependencyTree,
) -> Result<()> {
    if !options.include.is_match(file_path) || options.exclude.is_match(file_path) {
        let normalized_path = crate::utils::path::normalize_path_for_storage(file_path)?;
        tree.insert(normalized_path, None);
        return Ok(());
    }

    let path = Path::new(file_path);
    let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Resolve parser dynamically by extension. If no parser is available, insert empty deps.
    let parser_opt = crate::parser::ParserFactory::get_parser_for_extension(extension)?;
    if parser_opt.is_none() {
        let normalized_path = crate::utils::path::normalize_path_for_storage(file_path)?;
        tree.insert(normalized_path, Some(Vec::new()));
        return Ok(());
    }
    let parser = parser_opt.unwrap();

    let read_path_buf = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else if file_path.starts_with("../../") || file_path.starts_with("..\\..\\") {
        let cwd = std::env::current_dir().unwrap_or_default();
        let rest = &file_path[6..];
        let parent = cwd.parent().unwrap_or(&cwd);
        parent.join(rest)
    } else {
        options.context.join(file_path)
    };

    let content = crate::utils::read_file_text_async(&read_path_buf).await?;
    let dependencies = parser.parse_file(file_path, &content)?;

    let normalized_path = crate::utils::path::normalize_path_for_storage(file_path)?;
    tree.insert(normalized_path, Some(dependencies));
    Ok(())
}

// existing imports already include PathBuf
