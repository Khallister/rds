use crate::cache::FileCache;
use crate::types::ParseOptions;
use anyhow::Result;

type PartitionResult = (
    Vec<(String, Option<Vec<crate::types::Dependency>>)>,
    Vec<String>,
);

pub async fn partition_cached(
    cache: &mut FileCache,
    unprocessed: Vec<String>,
    _options: &ParseOptions,
) -> Result<PartitionResult> {
    let start = std::time::Instant::now();
    let mut cached_results = Vec::new();
    let mut files_to_parse = Vec::new();

    for file in unprocessed.into_iter() {
        let cache_key = crate::utils::path::normalize_path_for_storage_cached(&file).await?;
        if cache.is_cached(&file, &cache_key).await? {
            let deps_opt = cache.get_cached_dependencies(&cache_key);
            cached_results.push((file, deps_opt));
        } else {
            files_to_parse.push(file);
        }
    }

    if std::env::var("RDS_WATCH_DEBUG").is_ok() {
        let elapsed = start.elapsed();
        crate::logger::info(&format!(
            "partition_cached: total_inputs={}, cached={}, to_parse={} elapsed={}ms",
            cached_results.len() + files_to_parse.len(),
            cached_results.len(),
            files_to_parse.len(),
            elapsed.as_millis()
        ));
    }

    Ok((cached_results, files_to_parse))
}
