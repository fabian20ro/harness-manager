use std::path::PathBuf;

use anyhow::{Context, Result};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub home_dir: PathBuf,
    pub store_root: PathBuf,
    pub default_roots: Vec<PathBuf>,
    pub scan_max_depth: usize,
    pub known_global_dirs: Vec<PathBuf>,
    pub allowed_origins: Vec<String>,
    pub allow_insecure_doc_hosts: bool,
    pub max_snapshot_bytes: usize,
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
            home_dir.join(".gemini"),
            home_dir.join(".pi"),
            home_dir.join(".config").join("opencode"),
            home_dir.join(".config").join("antigravity"),
            home_dir.join(".github"),
        ];
        let mut allowed_origins = vec![
            "http://127.0.0.1:4173".to_string(),
            "http://localhost:4173".to_string(),
            "http://127.0.0.1:8765".to_string(),
            "http://localhost:8765".to_string(),
            "https://fabian20ro.github.io".to_string(),
        ];
        if let Ok(extra_origins) = std::env::var("HARNESS_ALLOWED_ORIGINS") {
            allowed_origins.extend(
                extra_origins
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string),
            );
        }
        allowed_origins.sort();
        allowed_origins.dedup();

        Ok(Self {
            home_dir,
            store_root,
            default_roots,
            scan_max_depth: 5,
            known_global_dirs,
            allowed_origins,
            allow_insecure_doc_hosts: std::env::var("HARNESS_ALLOW_INSECURE_DOC_HOSTS")
                .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            max_snapshot_bytes: 5_000_000,
        })
    }
}
