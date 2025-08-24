use crate::types::ParseOptions;
use anyhow::Result;
use glob::glob;
use regex::Regex;
use std::env;
use std::path::PathBuf;
use walkdir::WalkDir;

pub fn expand_entries(entries: &[String], options: &ParseOptions) -> Result<Vec<String>> {
    let mut found: Vec<PathBuf> = Vec::new();
    let context = &options.context;
    let ctx_abs = if context.is_absolute() {
        context.clone()
    } else {
        env::current_dir()?.join(context)
    };

    let exclusion_regex =
        Regex::new(r"node_modules|\.git|\.svn|\.hg|coverage|dist|build|out|\.next|\.nuxt")?;
    let include_re = &options.include;
    let exclude_re = &options.exclude;
    let default_exts = options.extensions.clone();

    let is_by_extension = |p: &std::path::Path| -> bool {
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e))
            .unwrap_or_else(|| "".to_string());
        default_exts.iter().any(|d| d == &ext)
    };

    let scan_dir = |dir: PathBuf, out: &mut Vec<PathBuf>| {
        for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if entry.file_type().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if exclusion_regex.is_match(name) {
                        continue;
                    }
                }
            } else if entry.file_type().is_file() {
                let file_s = p.to_string_lossy();
                if include_re.is_match(&file_s)
                    && !exclude_re.is_match(&file_s)
                    && is_by_extension(p)
                {
                    out.push(p.to_path_buf());
                }
            }
        }
    };

    for entry in entries.iter() {
        let contains_glob = entry.contains('*') || entry.contains('?') || entry.contains('[');

        if contains_glob {
            let pattern_path = ctx_abs.join(entry);
            let pattern_raw = pattern_path.to_string_lossy().to_string();
            let pattern = pattern_raw.replace('\\', "/");

            for m in glob(&pattern)? {
                if let Ok(p) = m {
                    if p.is_dir() {
                        scan_dir(p, &mut found);
                    } else if p.is_file() {
                        let file_s = p.to_string_lossy();
                        if include_re.is_match(&file_s)
                            && !exclude_re.is_match(&file_s)
                            && is_by_extension(&p)
                        {
                            found.push(p);
                        }
                    }
                }
            }
            continue;
        }

        let p = std::path::Path::new(entry);
        let p_abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            ctx_abs.join(p)
        };

        if p_abs.exists() && p_abs.is_dir() {
            scan_dir(p_abs, &mut found);
        } else if p_abs.exists() && p_abs.is_file() {
            let file_s = p_abs.to_string_lossy();
            if include_re.is_match(&file_s)
                && !exclude_re.is_match(&file_s)
                && is_by_extension(&p_abs)
            {
                found.push(p_abs);
            }
        } else {
            let maybe_abs = ctx_abs.join(p);
            if maybe_abs.exists() {
                if maybe_abs.is_dir() {
                    scan_dir(maybe_abs, &mut found);
                } else if maybe_abs.is_file() {
                    let file_s = maybe_abs.to_string_lossy();
                    if include_re.is_match(&file_s)
                        && !exclude_re.is_match(&file_s)
                        && is_by_extension(&maybe_abs)
                    {
                        found.push(maybe_abs);
                    }
                }
            } else {
                found.push(PathBuf::from(entry));
            }
        }
    }

    let mut out: Vec<String> = Vec::new();
    for p in found.into_iter() {
        if let Ok(rel) = p.strip_prefix(&ctx_abs) {
            let s = rel.to_string_lossy().replace('\\', "/");
            out.push(s);
        } else {
            out.push(p.to_string_lossy().replace('\\', "/"));
        }
    }

    out.sort();
    out.dedup();
    Ok(out)
}
