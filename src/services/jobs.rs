use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use chrono::Utc;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{domain::JobStatus, storage::Store};

#[derive(Clone)]
pub struct JobRegistry {
    jobs: Arc<Mutex<HashMap<String, JobStatus>>>,
    sender: broadcast::Sender<JobStatus>,
    store: Store,
}

impl JobRegistry {
    pub fn new(store: Store) -> Self {
        let (sender, _) = broadcast::channel(128);
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            sender,
            store,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<JobStatus> {
        self.sender.subscribe()
    }

    pub fn create(&self, kind: &str, message: &str) -> Result<JobStatus> {
        let job = JobStatus {
            id: Uuid::new_v4().to_string(),
            kind: kind.to_string(),
            status: "running".to_string(),
            created_at: Utc::now(),
            finished_at: None,
            message: message.to_string(),
        };
        self.jobs
            .lock()
            .expect("job registry poisoned")
            .insert(job.id.clone(), job.clone());
        self.store.write_json(&self.store.job_path(&job.id), &job)?;
        let _ = self.sender.send(job.clone());
        Ok(job)
    }

    pub fn finish(&self, mut job: JobStatus, status: &str, message: &str) -> Result<JobStatus> {
        job.status = status.to_string();
        job.message = message.to_string();
        job.finished_at = Some(Utc::now());
        self.jobs
            .lock()
            .expect("job registry poisoned")
            .insert(job.id.clone(), job.clone());
        self.store.write_json(&self.store.job_path(&job.id), &job)?;
        let _ = self.sender.send(job.clone());
        Ok(job)
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
}
