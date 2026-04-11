#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::TempDir;

    use crate::{
        config::AppConfig,
        domain::{GraphNode, ProjectKind},
        storage::Store,
    };

    use crate::services::scan::{
        scan_projects, scan_projects_with_progress,
    };
    use crate::services::jobs::JobRegistry;

    #[test]
    fn scan_finds_repo_and_codex_artifacts() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::write(repo.join("AGENTS.md"), "@./policy.md").expect("agents");
        fs::write(repo.join("policy.md"), "ok").expect("policy");

        let config = AppConfig {
            home_dir: home.clone(),
            store_root: temp.path().join("store"),
            default_roots: vec![home.join("git")],
            scan_max_depth: 5,
            known_global_dirs: vec![home.join(".codex")],
            allowed_origins: vec!["http://127.0.0.1:4173".to_string()],
            allow_insecure_doc_hosts: false,
            max_snapshot_bytes: 5_000_000,
        };
        let store = Store::new(config.store_root.clone());
        let jobs = JobRegistry::new(store.clone());
        let projects = scan_projects(&config, &store, &jobs, None).expect("scan");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].kind, ProjectKind::GitRepo);
        let state = store
            .read_json::<crate::domain::SurfaceState>(
                &store.tool_state_path(&projects[0].id, "codex"),
            )
            .expect("surface state");
        assert!(state
            .nodes
            .iter()
            .any(|node| matches!(node, GraphNode::Artifact(_))));
        assert!(state
            .edges
            .iter()
            .any(|edge| edge.reason.contains("Instruction import found")));
    }

    #[test]
    fn scan_reports_intermediate_progress() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::create_dir_all(repo.join("docs")).expect("docs dir");
        fs::write(repo.join("AGENTS.md"), "@./docs/policy.md").expect("agents");
        fs::write(repo.join("docs").join("policy.md"), "ok").expect("policy");

        let config = AppConfig {
            home_dir: home.clone(),
            store_root: temp.path().join("store"),
            default_roots: vec![home.join("git")],
            scan_max_depth: 5,
            known_global_dirs: vec![home.join(".codex")],
            allowed_origins: vec!["http://127.0.0.1:4173".to_string()],
            allow_insecure_doc_hosts: false,
            max_snapshot_bytes: 5_000_000,
        };
        let store = Store::new(config.store_root.clone());
        let jobs = JobRegistry::new(store.clone());
        let mut progress = Vec::new();

        let projects = scan_projects_with_progress(&config, &store, &jobs, None, |update| {
            progress.push(update);
            Ok(())
        })
        .expect("scan");

        assert_eq!(projects.len(), 1);
        assert!(progress.iter().any(|update| update.phase == "repo"));
        assert!(progress.iter().any(|update| update.phase == "walk"));
        assert!(progress
            .iter()
            .any(|update| update.current_path.as_deref() == Some("~/git/demo/docs")));
    }

    #[test]
    fn scan_discovers_workspace_candidates_from_known_global_dirs() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let workspace = home.join("scratch").join("notes-harness");
        fs::create_dir_all(&workspace).expect("workspace dir");
        fs::write(workspace.join("AGENTS.md"), "Read policy.md\n").expect("agents");

        let config = AppConfig {
            home_dir: home.clone(),
            store_root: temp.path().join("store"),
            default_roots: vec![home.join("git")],
            scan_max_depth: 5,
            known_global_dirs: vec![home.join("scratch")],
            allowed_origins: vec!["http://127.0.0.1:4173".to_string()],
            allow_insecure_doc_hosts: false,
            max_snapshot_bytes: 5_000_000,
        };
        let store = Store::new(config.store_root.clone());
        let jobs = JobRegistry::new(store.clone());
        let projects = scan_projects(&config, &store, &jobs, None).expect("scan");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].kind, ProjectKind::WorkspaceCandidate);
        assert_eq!(projects[0].root_path, workspace.to_string_lossy());
        assert!(projects[0].discovery_reason.contains("AGENTS.md"));
    }

    #[test]
    fn scan_ignores_weak_only_non_git_directories() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let weak = home.join("scratch").join("weak-only");
        fs::create_dir_all(weak.join(".github").join("hooks")).expect("hooks dir");
        fs::write(
            weak.join(".github").join("hooks").join("pre-tool-use.json"),
            "{}",
        )
        .expect("hooks");

        let config = AppConfig {
            home_dir: home.clone(),
            store_root: temp.path().join("store"),
            default_roots: vec![home.join("git")],
            scan_max_depth: 5,
            known_global_dirs: vec![home.join("scratch")],
            allowed_origins: vec!["http://127.0.0.1:4173".to_string()],
            allow_insecure_doc_hosts: false,
            max_snapshot_bytes: 5_000_000,
        };
        let store = Store::new(config.store_root.clone());
        let jobs = JobRegistry::new(store.clone());
        let projects = scan_projects(&config, &store, &jobs, None).expect("scan");
        assert!(projects.is_empty());
    }

    #[test]
    fn scan_discovers_copilot_skill_packages_from_global_github_root() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let skill_root = home.join(".github").join("skills").join("reviewer");
        fs::create_dir_all(&skill_root).expect("skill dir");
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: Reviewer\ndescription: test\n---\n",
        )
        .expect("skill");

        let config = AppConfig {
            home_dir: home.clone(),
            store_root: temp.path().join("store"),
            default_roots: vec![home.join("git")],
            scan_max_depth: 5,
            known_global_dirs: vec![home.join(".github")],
            allowed_origins: vec!["http://127.0.0.1:4173".to_string()],
            allow_insecure_doc_hosts: false,
            max_snapshot_bytes: 5_000_000,
        };
        let store = Store::new(config.store_root.clone());
        let jobs = JobRegistry::new(store.clone());
        let projects = scan_projects(&config, &store, &jobs, None).expect("scan");
        let reviewer = projects.iter().find(|p| p.name == "reviewer").expect("reviewer project found");
        assert_eq!(reviewer.kind, ProjectKind::PluginPackage);
        assert!(reviewer.discovery_reason.contains("SKILL.md"));
    }
}
