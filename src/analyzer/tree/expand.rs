use crate::types::ParseOptions;
use anyhow::Result;
use glob::glob;
use std::env;
use std::path::PathBuf;

fn is_by_extension(p: &std::path::Path, default_exts: &[String]) -> bool {
    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
        let ext_with_dot = format!(".{}", ext);
        default_exts.iter().any(|d| d == &ext_with_dot)
    } else {
        false
    }
}

fn pathbuf_to_normalized_string(p: &PathBuf) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub fn expand_entries(entries: &[String], options: &ParseOptions) -> Result<Vec<String>> {
    let mut found: Vec<PathBuf> = Vec::new();
    let context = &options.context;
    let ctx_abs = if context.is_absolute() {
        context.clone()
    } else {
        env::current_dir()?.join(context)
    };

    let exclude_re = &options.exclude;
    let include_re = &options.include;
    let default_exts = &options.extensions;
    crate::logger::debug(&format!("[expand_entries] default_exts={:?}", default_exts));

    fn scan_dir(
        dir: &std::path::Path,
        out: &mut Vec<std::path::PathBuf>,
        max_depth: usize,
        exclude_re: &regex::Regex,
        include_re: &regex::Regex,
        default_exts: &[String],
    ) {
        use walkdir::WalkDir;
        for entry in WalkDir::new(dir)
            .sort_by_file_name()
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p = entry.path();
            if entry.file_type().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if exclude_re.is_match(name) {
                        continue;
                    }
                }
            } else if entry.file_type().is_file() {
                let file_s = p.to_string_lossy();
                if include_re.is_match(&file_s) && !exclude_re.is_match(&file_s) && {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        let ext_with_dot = format!(".{}", ext);
                        default_exts.iter().any(|d| d == &ext_with_dot)
                    } else {
                        false
                    }
                } {
                    out.push(p.to_path_buf());
                }
            }
        }
    }

    let max_depth = options.max_depth;

    // Helper to resolve an entry to an absolute path relative to ctx_abs if not absolute
    fn resolve_to_abs(entry: &str, ctx_abs: &PathBuf) -> PathBuf {
        let p = std::path::Path::new(entry);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            ctx_abs.join(p)
        }
    }

    for entry in entries.iter() {
        let contains_glob = entry.contains('*') || entry.contains('?') || entry.contains('[');

        if contains_glob {
            // If entry is absolute, use as is; otherwise, join with ctx_abs
            let pattern_path = {
                let p = std::path::Path::new(entry);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    ctx_abs.join(p)
                }
            };
            // Use the native path representation for glob patterns
            let pattern = pattern_path.to_string_lossy();

            for m in glob(&pattern)? {
                if let Ok(p) = m {
                    if p.is_dir() {
                        scan_dir(
                            &p,
                            &mut found,
                            max_depth,
                            exclude_re,
                            include_re,
                            default_exts,
                        );
                    } else if p.is_file() {
                        let file_s = p.to_string_lossy();
                        if include_re.is_match(&file_s)
                            && !exclude_re.is_match(&file_s)
                            && is_by_extension(&p, default_exts)
                        {
                            found.push(p);
                        }
                    }
                }
            }
            continue;
        }

        let p_abs = resolve_to_abs(entry, &ctx_abs);

        if p_abs.exists() && p_abs.is_dir() {
            scan_dir(
                &p_abs,
                &mut found,
                max_depth,
                exclude_re,
                include_re,
                default_exts,
            );
        } else if p_abs.exists() && p_abs.is_file() {
            let file_s = p_abs.to_string_lossy();
            if include_re.is_match(&file_s)
                && !exclude_re.is_match(&file_s)
                && is_by_extension(&p_abs, default_exts)
            {
                found.push(p_abs);
            }
        } else {
            // If the entry does not exist in the filesystem and was not matched by a glob pattern,
            // ensure the path is made absolute relative to ctx_abs for consistency.
            // This ensures downstream consumers (such as build tools or analyzers) receive paths that are always absolute with respect to the project context,
            // even if the files do not exist yet, avoiding confusion or errors when handling missing files in later processing stages.
            found.push(p_abs);
        }
    }

    use std::collections::HashSet;

    // Deduplicate using HashSet<PathBuf> first
    let mut out_set: HashSet<PathBuf> = HashSet::new();
    for p in found.into_iter() {
        // Normalize to relative path if possible, else keep as absolute
        if let Ok(rel) = p.strip_prefix(&ctx_abs) {
            out_set.insert(rel.to_path_buf());
        } else {
            out_set.insert(p);
        }
    }

    // Convert to normalized String only once at the end
    let mut out: Vec<String> = out_set
        .into_iter()
        .map(|p| pathbuf_to_normalized_string(&p))
        .collect();
    // Sorting ensures deterministic output order, which is important for reproducibility and downstream consumers.
    out.sort();

    Ok(out)
}
