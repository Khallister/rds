use crate::cache::FileCache;
use crate::types::ParseOptions;
use anyhow::Result;

pub async fn partition_cached(
    cache: &mut FileCache,
    unprocessed: Vec<String>,
    _options: &ParseOptions,
) -> Result<(
    Vec<(String, Option<Vec<crate::types::Dependency>>)>,
    Vec<String>,
)> {
    let mut cached_results = Vec::new();
    let mut files_to_parse = Vec::new();

    for file in unprocessed.into_iter() {
        let cache_key = crate::utils::path::normalize_path_for_storage(&file)?;
        if cache.is_cached(&file, &cache_key).await? {
            let deps_opt = cache.get_cached_dependencies(&cache_key);
            cached_results.push((file, deps_opt));
        } else {
            files_to_parse.push(file);
        }
    }

    Ok((cached_results, files_to_parse))
}
