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
        self.root.join("projects").join(project_id)
    }

    pub fn tool_state_path(&self, project_id: &str, tool: &str) -> PathBuf {
        self.project_dir(project_id)
            .join("tool-state")
            .join(format!("{tool}.json"))
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
        self.root.join("snapshots").join(snapshot_id)
    }

    pub fn activity_path(&self, project_id: &str, tool: &str) -> PathBuf {
        self.root
            .join("activity")
            .join(project_id)
            .join(format!("{tool}.json"))
    }

    pub fn job_path(&self, job_id: &str) -> PathBuf {
        self.root.join("jobs").join(format!("{job_id}.json"))
    }

    pub fn edit_backup_path(&self, project_id: &str, node_id: &str) -> PathBuf {
        self.root
            .join("edit-backups")
            .join(project_id)
            .join(format!("{node_id}.json"))
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
