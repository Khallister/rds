use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct FileConfig {
    pub extensions: Option<Vec<String>>,
    pub include: Option<String>,
    pub exclude: Option<String>,
    pub dependency_exclude: Option<String>,
    pub cache_enabled: Option<bool>,
    pub take: Option<usize>,
    pub tsconfig: Option<String>,
}

// Load rds.config.toml from project root or user config directory.
// Non-fatal: on error or missing file, returns None.
pub fn load_rds_config() -> Option<FileConfig> {
    // If XDG_CONFIG_HOME (or HOME/.config) is set, prefer project-specific config there so tests
    // and CI can override repo files without changing CWD.
    if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        let cfg_dir = PathBuf::from(xdg).join("rds");
        let p = cfg_dir.join("rds.config.toml");
        if p.exists() {
            match fs::read_to_string(&p) {
                Ok(s) => match toml::from_str::<FileConfig>(&s) {
                    Ok(cfg) => return Some(cfg),
                    Err(e) => eprintln!("[rds config] failed to parse {}: {}", p.display(), e),
                },
                Err(e) => eprintln!("[rds config] failed to read {}: {}", p.display(), e),
            }
        }
    } else if let Ok(home) = env::var("HOME") {
        let cfg_dir = PathBuf::from(home).join(".config").join("rds");
        let p = cfg_dir.join("rds.config.toml");
        if p.exists() {
            match fs::read_to_string(&p) {
                Ok(s) => match toml::from_str::<FileConfig>(&s) {
                    Ok(cfg) => return Some(cfg),
                    Err(e) => eprintln!("[rds config] failed to parse {}: {}", p.display(), e),
                },
                Err(e) => eprintln!("[rds config] failed to read {}: {}", p.display(), e),
            }
        }
    }

    // Fallback: project root ./rds.config.toml
    if let Ok(cwd) = env::current_dir() {
        let p = cwd.join("rds.config.toml");
        if p.exists() {
            match fs::read_to_string(&p) {
                Ok(s) => match toml::from_str::<FileConfig>(&s) {
                    Ok(cfg) => return Some(cfg),
                    Err(e) => eprintln!("[rds config] failed to parse {}: {}", p.display(), e),
                },
                Err(e) => eprintln!("[rds config] failed to read {}: {}", p.display(), e),
            }
        }
    }

    None
}
