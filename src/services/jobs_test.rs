#[cfg(test)]
mod tests {
    use crate::services::jobs::{JobRegistry, JobUpdate};
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

    #[tokio::test]
    async fn test_get_returns_job_if_exists() {
        let temp = TempDir::new().unwrap();
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let job = registry.create("scan", "Scanning...").expect("job");
        let found = registry.get(&job.id).unwrap().unwrap();
        assert_eq!(found.id, job.id);
    }

    #[tokio::test]
    async fn test_get_returns_none_if_not_found() {
        let temp = TempDir::new().unwrap();
        let registry = JobRegistry::new(Store::new(temp.path().join("store")));
        let found = registry.get("non-existent-id").unwrap();
        assert!(found.is_none());
    }
}
