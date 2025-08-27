use crate::cache::FileCache;
use crate::logger;
use crate::parser::{DynParser, ModuleResolver};
use crate::types::{Dependency, DependencyTree, ParseOptions};
use anyhow::{Error, Result};
use futures::stream::StreamExt;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
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
        logger::debug("[Batch] Parsed one file");
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

    // Shared read + parse flow
    let (normalized_path, dependencies) =
        read_and_parse_file(file_path, parser.clone(), options).await?;
    logger::info(&format!(
        "Parsed file static: {} (parse: {}ms)",
        normalized_path, 0
    ));

    if let Some(ref callback) = options.progress_callback {
        callback(crate::types::ProgressEvent::End, file_path);
    }

    Ok((normalized_path.to_string(), Some(dependencies)))
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
    let proc_start = Instant::now();
    let parsed_count = parsed_results.len();
    for result in parsed_results {
        match result {
            Ok((file_path, dependencies_opt)) => {
                if let Some(mut deps) = dependencies_opt {
                    let cache_key =
                        crate::utils::path::normalize_path_for_storage_cached(&file_path).await?;

                    // Resolve dependency requests to absolute/normalized ids when possible.
                    // Collect unresolved requests and resolve them in parallel to avoid
                    // serial awaits per dependency.
                    let context = Path::new(&file_path).parent().unwrap_or(Path::new("."));
                    let mut unresolved_tasks: Vec<(usize, String)> = Vec::new();
                    for (i, dep) in deps.iter().enumerate() {
                        if dep.id.is_none() {
                            unresolved_tasks.push((i, dep.request.clone()));
                        }
                    }

                    if !unresolved_tasks.is_empty() {
                        use futures::stream::{self, StreamExt};
                        let exts = options.extensions.clone();
                        let resolver_ref = resolver;
                        let results: Vec<(usize, Option<String>)> = stream::iter(unresolved_tasks)
                            .map(|(i, request)| {
                                let resolver = resolver_ref;
                                let ctx = context.to_path_buf();
                                let exts = exts.clone();
                                async move {
                                    match resolver.resolve_module(&ctx, &request, &exts).await {
                                        Ok(Some(resolved_path)) => {
                                            let norm = crate::utils::path::normalize_path_for_storage_cached(&resolved_path).await.ok();
                                            (i, norm)
                                        }
                                        _ => (i, None),
                                    }
                                }
                            })
                            .buffer_unordered(32)
                            .collect()
                            .await;

                        for (i, norm_opt) in results {
                            if let Some(norm) = norm_opt {
                                if let Some(dep) = deps.get_mut(i) {
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
                        crate::utils::path::normalize_path_for_storage_cached(&file_path).await?;
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
                            let normalized = crate::utils::path::normalize_path_for_storage_cached(
                                &resolved_path,
                            )
                            .await?;
                            if !processed_files.contains(&normalized)
                                && !new_dependencies.contains(&normalized)
                            {
                                new_dependencies.push(normalized);
                            }
                        }
                    }
                } else {
                    let normalized_path =
                        crate::utils::path::normalize_path_for_storage_cached(&file_path).await?;
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
    let elapsed = proc_start.elapsed();
    crate::logger::info(&format!(
        "process_parsed_results: processed {} parsed results, new_deps_added={}, elapsed={}ms",
        parsed_count,
        new_dependencies.len(),
        elapsed.as_millis()
    ));
    Ok(())
}

pub async fn parse_single_file_deps(
    _cache: &mut FileCache,
    file_path: &str,
    options: &ParseOptions,
    tree: &mut DependencyTree,
) -> Result<()> {
    if !options.include.is_match(file_path) || options.exclude.is_match(file_path) {
        let normalized_path =
            crate::utils::path::normalize_path_for_storage_cached(file_path).await?;
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

    let (normalized_path, dependencies) =
        read_and_parse_file(file_path, parser.clone(), options).await?;
    logger::info(&format!(
        "Parsed file deps: {} (parse: {}ms)",
        normalized_path, 0
    ));

    let normalized_storage =
        crate::utils::path::normalize_path_for_storage_cached(&normalized_path).await?;
    tree.insert(normalized_storage, Some(dependencies));
    Ok(())
}

async fn read_and_parse_file(
    file_path: &str,
    parser: DynParser,
    options: &ParseOptions,
) -> Result<(String, Vec<Dependency>)> {
    // Compute absolute read path
    let read_path_buf = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else {
        let ctx_abs = if options.context == PathBuf::from(".") {
            std::env::current_dir().unwrap_or_default()
        } else if options.context.is_absolute() {
            options.context.clone()
        } else {
            std::env::current_dir()
                .unwrap_or_default()
                .join(&options.context)
        };

        let mut ancestor = ctx_abs.clone();
        use std::path::Component;
        let comps: Vec<Component> = Path::new(file_path).components().collect();
        let mut skip = 0usize;
        while skip < comps.len() {
            logger::debug(&format!(
                "[Path Handling] Component[{}]: {:?}",
                skip, comps[skip]
            ));
            match comps[skip] {
                Component::ParentDir => {
                    if let Some(parent) = ancestor.parent() {
                        ancestor = parent.to_path_buf();
                    }
                    skip += 1;
                }
                _ => break,
            }
        }

        let mut remaining = PathBuf::new();
        for c in comps.into_iter().skip(skip) {
            match c {
                Component::Normal(os) => remaining.push(os),
                Component::Prefix(p) => remaining.push(p.as_os_str()),
                Component::RootDir => remaining.push(std::path::MAIN_SEPARATOR.to_string()),
                Component::CurDir => {}
                Component::ParentDir => remaining.push(".."),
            }
        }

        if remaining.as_os_str().is_empty() {
            ancestor
        } else {
            ancestor.join(remaining)
        }
    };

    let read_start = Instant::now();
    logger::info(&format!("Reading file: {}", read_path_buf.display()));
    let content = crate::utils::read_file_text_async(&read_path_buf).await?;
    let read_dur = read_start.elapsed();
    logger::info(&format!(
        "Read {} bytes: {} (read: {}ms)",
        content.len(),
        read_path_buf.display(),
        read_dur.as_millis()
    ));

    let normalized_path = crate::utils::path::canonicalize_cached(&read_path_buf).await;

    logger::info(&format!("Parsing file: {}", normalized_path));
    let parser_clone = parser.clone();
    let normalized_clone = normalized_path.clone();
    let content_clone = content.clone();
    let parse_start = Instant::now();
    let dependencies = tokio::task::spawn_blocking(move || {
        parser_clone.parse_file(&normalized_clone, &content_clone)
    })
    .await
    .map_err(|e| anyhow::anyhow!("Parser thread join error: {}", e))??;
    let _parse_dur = parse_start.elapsed();

    Ok((normalized_path, dependencies))
}

// existing imports already include PathBuf
