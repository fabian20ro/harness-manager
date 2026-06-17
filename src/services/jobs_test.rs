#[cfg(test)]
mod tests {
    use crate::services::jobs::{JobRegistry, JobUpdate};
    use crate::storage::Store;
    use tempfile::TempDir;
    use chrono::Utc;

    #[test]
    fn test_create_scoped_sets_all_fields() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry
            .create_scoped("scan", "Scanning...", Some("global"), Some("project1"), Some("tool1"))
            .expect("job");

        assert_eq!(job.kind, "scan");
        assert_eq!(job.scope_kind.as_deref(), Some("global"));
        assert_eq!(job.project_id.as_deref(), Some("project1"));
        assert_eq!(job.tool.as_deref(), Some("tool1"));
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
        assert_eq!(updated.progress, Some(1.0 / 3.0));
    }

    #[test]
    fn test_update_sets_progress() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let mut job = registry.create("scan", "Scanning...").expect("job");
        job.items_total = Some(10);

        let updated = registry.update(
            job.clone(),
            JobUpdate {
                items_done: Some(Some(5)),
                ..JobUpdate::default()
            },
        ).expect("updated");

        assert_eq!(updated.progress, Some(0.5));
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

    #[tokio::test]
    async fn test_get_returns_job_if_exists() {
        let temp = TempDir::new().unwrap();
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning...").expect("job");
        let found = registry.get(&job.id).unwrap().unwrap();
        assert_eq!(found.id, job.id);
    }

    #[test]
    fn test_update_error_on_invalid_progress() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning projects.").expect("job");

        let result = registry.update(
            job.clone(),
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
    fn test_finish_error_on_non_running_job() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let mut job = registry.create("scan", "Scanning...").expect("job");
        job.status = "completed".to_string();

        let result = registry.finish(job, "completed", "Done.");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot finish a job that is not running"));
    }

    #[test]
    fn test_update_with_missing_total_does_not_set_progress() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let mut job = registry.create("scan", "Scanning...").expect("job");
        job.items_total = None;
        job.items_done = Some(5);

        let updated = registry.update(
            job.clone(),
            JobUpdate {
                items_done: Some(Some(5)),
                ..JobUpdate::default()
            },
        ).expect("updated");

        assert!(updated.progress.is_none());
    }

    #[test]
    fn test_finish_sets_progress_to_one() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let mut job = registry.create("scan", "Scanning...").expect("job");
        job.items_total = Some(10);
        job.items_done = Some(5);
        
        let finished = registry
            .finish(job.clone(), "completed", "Done.")
            .expect("finish job");
            
        assert_eq!(finished.progress, Some(1.0));
        assert_eq!(finished.status, "completed");

        let persisted = registry.get(&job.id).unwrap().expect("persisted job");
        assert_eq!(persisted.progress, Some(1.0));
    }

    #[test]
    fn test_update_with_zero_total_sets_progress_to_one() {
        let temp = TempDir::new().expect("tempdir");
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let mut job = registry.create("scan", "Scanning...").expect("job");
        job.items_total = Some(0);
        job.items_done = Some(0);
        
        let finished = registry
            .finish(job.clone(), "completed", "Done.")
            .expect("finish job");
            
        assert_eq!(finished.progress, Some(1.0));
        assert_eq!(finished.status, "completed");
    }
}