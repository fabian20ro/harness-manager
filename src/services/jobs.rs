use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use chrono::Utc;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::{domain::JobStatus, storage::Store};

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    #[allow(dead_code)]
    tx: mpsc::UnboundedSender<PathBuf>,
}

impl FileWatcher {
    pub fn new<F>(on_event: F) -> Result<Self>
    where
        F: Fn(PathBuf) + Send + 'static,
    {
        let (tx, mut rx) = mpsc::unbounded_channel::<PathBuf>();
        let event_tx = tx.clone();

        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    for path in event.paths {
                        let _ = event_tx.send(path);
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )?;

        tokio::spawn(async move {
            while let Some(path) = rx.recv().await {
                on_event(path);
            }
        });

        Ok(Self { watcher, tx })
    }

    pub fn watch(&mut self, path: &Path) -> Result<()> {
        self.watcher
            .watch(path, RecursiveMode::Recursive)
            .context("failed to start watching path")
    }

    pub fn unwatch(&mut self, path: &Path) -> Result<()> {
        self.watcher
            .unwatch(path)
            .context("failed to stop watching path")
    }
}

#[derive(Clone)]
pub struct JobRegistry {
    jobs: Arc<Mutex<HashMap<String, JobStatus>>>,
    sender: broadcast::Sender<JobStatus>,
    store: Store,
    watcher: Arc<Mutex<Option<FileWatcher>>>,
}

#[derive(Clone, Debug, Default)]
pub struct JobUpdate {
    pub status: Option<String>,
    pub message: Option<String>,
    pub scope_kind: Option<Option<String>>,
    pub project_id: Option<Option<String>>,
    pub tool: Option<Option<String>>,
    pub phase: Option<Option<String>>,
    pub current_path: Option<Option<String>>,
    pub items_done: Option<Option<usize>>,
    pub items_total: Option<Option<usize>>,
}

impl JobRegistry {
    pub fn new(store: Store) -> Self {
        let (sender, _) = broadcast::channel(128);
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            sender,
            store,
            watcher: Arc::new(Mutex::new(None)),
        }
    }

    pub fn setup_watcher<F>(&self, on_event: F) -> Result<()>
    where
        F: Fn(PathBuf) + Send + 'static,
    {
        let mut watcher_lock = self.watcher.lock().expect("watcher lock poisoned");
        if watcher_lock.is_none() {
            *watcher_lock = Some(FileWatcher::new(on_event)?);
        }
        Ok(())
    }

    pub fn watch_path(&self, path: &Path) -> Result<()> {
        let mut watcher_lock = self.watcher.lock().expect("watcher lock poisoned");
        if let Some(watcher) = watcher_lock.as_mut() {
            watcher.watch(path)?;
        }
        Ok(())
    }

    pub fn unwatch_path(&self, path: &Path) -> Result<()> {
        let mut watcher_lock = self.watcher.lock().expect("watcher lock poisoned");
        if let Some(watcher) = watcher_lock.as_mut() {
            watcher.unwatch(path)?;
        }
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<JobStatus> {
        self.sender.subscribe()
    }

    pub fn create(&self, kind: &str, message: &str) -> Result<JobStatus> {
        self.create_scoped(kind, message, None, None, None)
    }

    pub fn create_scoped(
        &self,
        kind: &str,
        message: &str,
        scope_kind: Option<&str>,
        project_id: Option<&str>,
        tool: Option<&str>,
    ) -> Result<JobStatus> {
        let job = JobStatus {
            id: Uuid::new_v4().to_string(),
            kind: kind.to_string(),
            status: "running".to_string(),
            created_at: Utc::now(),
            finished_at: None,
            message: message.to_string(),
            scope_kind: scope_kind.map(ToString::to_string),
            project_id: project_id.map(ToString::to_string),
            tool: tool.map(ToString::to_string),
            phase: None,
            current_path: None,
            items_done: None,
            items_total: None,
        };
        self.jobs
            .lock()
            .expect("job registry poisoned")
            .insert(job.id.clone(), job.clone());
        self.store.write_json(&self.store.job_path(&job.id), &job)?;
        let _ = self.sender.send(job.clone());
        Ok(job)
    }

    pub fn update(&self, mut job: JobStatus, patch: JobUpdate) -> Result<JobStatus> {
        if let Some(status) = patch.status {
            job.status = status;
        }
        if let Some(message) = patch.message {
            job.message = message;
        }
        if let Some(scope_kind) = patch.scope_kind {
            job.scope_kind = scope_kind;
        }
        if let Some(project_id) = patch.project_id {
            job.project_id = project_id;
        }
        if let Some(tool) = patch.tool {
            job.tool = tool;
        }
        if let Some(phase) = patch.phase {
            job.phase = phase;
        }
        if let Some(current_path) = patch.current_path {
            job.current_path = current_path;
        }
        if let Some(items_done) = patch.items_done {
            job.items_done = items_done;
        }
        if let Some(items_total) = patch.items_total {
            job.items_total = items_total;
        }
        self.jobs
            .lock()
            .expect("job registry poisoned")
            .insert(job.id.clone(), job.clone());
        self.store.write_json(&self.store.job_path(&job.id), &job)?;
        let _ = self.sender.send(job.clone());
        Ok(job)
    }

    pub fn finish(&self, mut job: JobStatus, status: &str, message: &str) -> Result<JobStatus> {
        job.finished_at = Some(Utc::now());
        self.update(
            job,
            JobUpdate {
                status: Some(status.to_string()),
                message: Some(message.to_string()),
                ..JobUpdate::default()
            },
        )
    }

    pub fn get(&self, job_id: &str) -> Result<Option<JobStatus>> {
        if let Some(job) = self
            .jobs
            .lock()
            .expect("job registry poisoned")
            .get(job_id)
            .cloned()
        {
            return Ok(Some(job));
        }
        self.store.maybe_read_json(&self.store.job_path(job_id))
    }

    pub fn find_running_kind(&self, kind: &str) -> Option<JobStatus> {
        self.jobs
            .lock()
            .expect("job registry poisoned")
            .values()
            .find(|job| job.kind == kind && job.status == "running")
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use crate::storage::Store;

    use super::{JobRegistry, JobUpdate};

    #[test]
    fn update_keeps_job_identity_and_running_state() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning projects.").expect("job");

        let updated = registry
            .update(
                job.clone(),
                JobUpdate {
                    message: Some("Scanning ~/git/demo".to_string()),
                    scope_kind: Some(Some("global".to_string())),
                    phase: Some(Some("repo".to_string())),
                    current_path: Some(Some("~/git/demo".to_string())),
                    items_done: Some(Some(1)),
                    items_total: Some(Some(3)),
                    ..JobUpdate::default()
                },
            )
            .expect("updated");

        assert_eq!(updated.id, job.id);
        assert_eq!(updated.status, "running");
        assert_eq!(updated.scope_kind.as_deref(), Some("global"));
        assert_eq!(updated.phase.as_deref(), Some("repo"));
        assert_eq!(updated.current_path.as_deref(), Some("~/git/demo"));
        assert_eq!(updated.items_done, Some(1));
        assert_eq!(updated.items_total, Some(3));
    }

    #[test]
    fn find_running_kind_returns_only_running_jobs() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let scan_job = registry.create("scan", "Scanning projects.").expect("scan job");
        let other_job = registry
            .create("refresh-activity", "Refreshing activity.")
            .expect("other job");
        registry
            .finish(other_job, "completed", "Done.")
            .expect("finish other job");

        let found = registry.find_running_kind("scan").expect("running scan");
        assert_eq!(found.id, scan_job.id);
        assert!(registry.find_running_kind("refresh-activity").is_none());
    }
}
