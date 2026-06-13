#[cfg(test)]
mod tests {
    use crate::services::jobs::{JobRegistry, JobUpdate};
    use crate::domain::JobStatus;
    use crate::storage::Store;
    use tempfile::TempDir;
    use chrono::Utc;

#[tokio::test]
    async fn test_setup_watcher_returns_self() {
        let temp = TempDir::new().unwrap();
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let result = registry.setup_watcher(|_| {});
        assert!(result.is_ok());
        let registry_ref = result.unwrap();
        assert!(matches!(registry_ref, JobRegistry { .. }));
    }

    #[test]
    fn test_create_sets_defaults_for_optional_fields() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning...").expect("job");

        assert_eq!(job.kind, "scan");
        assert_eq!(job.status, "running");
        assert_eq!(job.message, "Scanning...");
        assert!(job.finished_at.is_none());
        assert!(job.scope_kind.is_none());
        assert!(job.project_id.is_none());
        assert!(job.tool.is_none());
        assert!(job.phase.is_none());
    }

    #[test]
    fn test_create_scoped_sets_project_and_tool() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry
            .create_scoped("scan", "Scanning...", None, Some("global"), Some("claude-code"))
            .expect("job");

        assert_eq!(job.kind, "scan");
        assert_eq!(job.project_id.as_deref(), Some("global"));
        assert_eq!(job.tool.as_deref(), Some("claude-code"));
    }

    #[test]
    fn test_update_keeps_job_identity_and_running_state() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning projects.").expect("job");

        let updated = registry.update(
            job.clone(),
            JobUpdate {
                message: Some("New message".to_string()),
                scope_kind: Some(Some("global".to_string())),
                phase: Some(Some("repo".to_string())),
                current_path: Some(Some("~/git/demo".to_string())),
                items_done: Some(Some(1)),
                items_total: Some(Some(3)),
                ..JobUpdate::default()
            },
        ).expect("updated");

        assert_eq!(updated.id, job.id);
        assert_eq!(updated.status, "running");
        assert_eq!(updated.scope_kind.as_deref(), Some("global"));
        assert_eq!(updated.phase.as_deref(), Some("repo"));
        assert_eq!(updated.current_path.as_deref(), Some("~/git/demo"));
        assert_eq!(updated.items_done, Some(1));
        assert_eq!(updated.items_total, Some(3));
    }

    #[test]
    fn test_find_running_kind_returns_only_running_jobs() {
        let temp = TempDir::new().expect("tempdir");
        let store = Store::new(temp.path().join("store"));
        let registry = JobRegistry::new(store.clone());
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

    #[test]
    fn test_create_sets_created_at() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning...").expect("job");

        assert!(job.created_at <= Utc::now());
    }

    #[test]
    fn test_update_errors_on_impossible_progress() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning projects.").expect("job");

        let result = registry.update(
            job,
            JobUpdate {
                items_done: Some(Some(5)),
                items_total: Some(Some(3)),
                ..JobUpdate::default()
            },
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be greater than"));
    }

    #[test]
    fn test_finish_sets_finished_at() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning...").expect("job");
        let start_time = Utc::now();

        let finished = registry
            .finish(job, "completed", "Done.")
            .expect("finish job");
        assert!(finished.finished_at.is_some());
        assert!(finished.finished_at.unwrap() >= start_time);
    }

    #[test]
    fn test_finish_sets_finished_at_v2() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning...").expect("job");
        let start_time = Utc::now();

        let finished_job = registry
            .finish(job, "completed", "Done.")
            .expect("finish job");
        assert_eq!(finished_job.status, "completed");
        assert_eq!(finished_job.message, "Done.");
        assert!(finished_job.finished_at.is_some());
        assert!(finished_job.finished_at.unwrap() >= start_time);
    }

    #[test]
    fn test_update_prevents_invalid_items_counts() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let mut job = registry.create("scan", "Scanning...").expect("job");
        job.items_total = Some(10);
        job.items_done = Some(5);

        let result = registry.update(
            job.clone(),
            JobUpdate {
                items_done: Some(Some(11)),
                ..JobUpdate::default()
            },
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_update_job_status_and_metadata() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Initial message").expect("job");
        
        let updated = registry.update(
            job.clone(),
            JobUpdate {
                status: Some("running".to_string()),
                message: Some("New message".to_string()),
                phase: Some(Some("indexing".to_string())),
                ..JobUpdate::default()
            },
        ).expect("update");

        assert_eq!(updated.message, "New message");
        assert_eq!(updated.phase, Some("indexing".to_string()));
        assert_eq!(updated.status, "running");
        assert_eq!(updated.id, job.id);
    }

    #[test]
    fn test_find_running_kind_fallback_to_store() {
        let temp = TempDir::new().expect("tempdir");
        let store = Store::new(temp.path().join("store"));
        let registry = JobRegistry::new(store.clone());
        let job = registry.create("scan", "Scanning...").expect("scan job");
        
        let registry_new = JobRegistry::new(store);
        let found = registry_new.find_running_kind("scan").expect("running scan");
        assert_eq!(found.id, job.id);
        assert_eq!(found.status, "running");
    }
}
