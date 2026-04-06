use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::domain::NodeState;

pub fn stable_id(prefix: &str, input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{prefix}:{:x}", hasher.finalize())
}

pub fn file_hash(path: &Path) -> Result<String> {
    if path.is_dir() {
        return Ok("directory".to_string());
    }
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn mtime_utc(metadata: fs::Metadata) -> Option<DateTime<Utc>> {
    metadata.modified().ok().map(DateTime::<Utc>::from)
}

pub fn confidence_from_states(states: &[NodeState]) -> f32 {
    if states.contains(&NodeState::Observed) {
        0.98
    } else if states.contains(&NodeState::Effective) {
        0.92
    } else if states.contains(&NodeState::BrokenReference) {
        0.84
    } else {
        0.7
    }
}

pub fn resolve_catalog_path(pattern: &str, home_dir: &Path, project_root: Option<&Path>) -> PathBuf {
    let with_home = pattern.replace('~', &home_dir.to_string_lossy());
    let with_project = if let Some(project_root) = project_root {
        with_home.replace("{project}", &project_root.to_string_lossy())
    } else {
        with_home
    };
    PathBuf::from(with_project)
}
