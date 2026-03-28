use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub home_dir: PathBuf,
    pub store_root: PathBuf,
    pub default_roots: Vec<PathBuf>,
    pub scan_max_depth: usize,
    pub known_global_dirs: Vec<PathBuf>,
}

impl AppConfig {
    pub fn default() -> Result<Self> {
        let home_dir = dirs::home_dir().context("home directory not found")?;
        let store_root = home_dir.join(".harness-inspector");
        let default_roots = vec![home_dir.join("git")];
        let known_global_dirs = vec![
            home_dir.join(".codex"),
            home_dir.join(".claude"),
            home_dir.join(".config").join("claude"),
            home_dir.join(".config").join("opencode"),
            home_dir.join(".config").join("antigravity"),
            home_dir.join(".github"),
        ];

        Ok(Self {
            home_dir,
            store_root,
            default_roots,
            scan_max_depth: 5,
            known_global_dirs,
        })
    }
}
