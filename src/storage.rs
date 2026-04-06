use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Serialize};

#[derive(Clone, Debug)]
pub struct Store {
    pub root: PathBuf,
}

fn sanitize(name: &str) -> String {
    let mut safe = Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();

    // Path::file_name() returns None for inputs ending in "." or ".."
    if safe.is_empty() {
        safe = name.replace("..", "_").replace('.', "_").replace('/', "_");
    }

    // Path::file_name() does not recognize backslashes as path separators on Unix,
    // so we manually replace backslashes to ensure no trailing path parts remain.
    if let Some(last) = safe.split('\\').last() {
        safe = last.to_string();
    }

    // Final fallback in case of weird edge cases
    if safe.is_empty() {
        safe = "invalid_name".to_string();
    }

    safe
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn ensure_layout(&self) -> Result<()> {
        fs::create_dir_all(self.root.join("catalogs"))?;
        fs::create_dir_all(self.root.join("projects"))?;
        fs::create_dir_all(self.root.join("snapshots"))?;
        fs::create_dir_all(self.root.join("activity"))?;
        fs::create_dir_all(self.root.join("jobs"))?;
        fs::create_dir_all(self.root.join("edit-backups"))?;
        Ok(())
    }

    pub fn projects_index_path(&self) -> PathBuf {
        self.root.join("projects").join("index.json")
    }

    pub fn project_dir(&self, project_id: &str) -> PathBuf {
        self.root.join("projects").join(sanitize(project_id))
    }

    pub fn tool_state_path(&self, project_id: &str, tool: &str) -> PathBuf {
        self.project_dir(project_id)
            .join("tool-state")
            .join(format!("{}.json", sanitize(tool)))
    }

    pub fn graph_nodes_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("graph.nodes.json")
    }

    pub fn graph_edges_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("graph.edges.json")
    }

    pub fn inventory_path(&self, project_id: &str) -> PathBuf {
        self.project_dir(project_id).join("inventory.json")
    }

    pub fn snapshot_dir(&self, snapshot_id: &str) -> PathBuf {
        self.root.join("snapshots").join(sanitize(snapshot_id))
    }

    pub fn activity_path(&self, project_id: &str, tool: &str) -> PathBuf {
        self.root
            .join("activity")
            .join(sanitize(project_id))
            .join(format!("{}.json", sanitize(tool)))
    }

    pub fn job_path(&self, job_id: &str) -> PathBuf {
        self.root.join("jobs").join(format!("{}.json", sanitize(job_id)))
    }

    pub fn edit_backup_path(&self, project_id: &str, node_id: &str) -> PathBuf {
        self.root
            .join("edit-backups")
            .join(sanitize(project_id))
            .join(format!("{}.json", sanitize(node_id)))
    }

    pub fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn read_json<T: DeserializeOwned>(&self, path: &Path) -> Result<T> {
        let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn maybe_read_json<T: DeserializeOwned>(&self, path: &Path) -> Result<Option<T>> {
        if !path.exists() {
            return Ok(None);
        }
        self.read_json(path).map(Some)
    }

    pub fn write_text_atomic(&self, path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, content)?;
        fs::rename(tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_sanitize_prevents_path_traversal() {
        assert_eq!(sanitize("valid_name"), "valid_name");
        assert_eq!(sanitize("../../../etc/passwd"), "passwd");
        assert_eq!(sanitize("some/path/to/file.json"), "file.json");
        assert_eq!(sanitize("C:\\Windows\\System32\\cmd.exe"), "cmd.exe");
    }

    #[test]
    fn test_store_paths_are_safe() {
        let store = Store::new(PathBuf::from("/tmp/store"));

        // Even with malicious input, the resulting path should remain within the expected directory
        let malicious_id = "../../../malicious";

        let project_dir = store.project_dir(malicious_id);
        assert_eq!(project_dir, PathBuf::from("/tmp/store/projects/malicious"));

        let tool_state = store.tool_state_path(malicious_id, "../../../etc/passwd");
        assert_eq!(tool_state, PathBuf::from("/tmp/store/projects/malicious/tool-state/passwd.json"));

        // When input is just traversal tokens
        let traversal_id = "../../..";
        let project_dir_traversal = store.project_dir(traversal_id);
        assert_eq!(project_dir_traversal, PathBuf::from("/tmp/store/projects/_____"));
    }
}
