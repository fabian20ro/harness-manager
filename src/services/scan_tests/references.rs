#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::TempDir;

    use crate::{
        catalogs::seed_catalog_map,
        config::AppConfig,
        domain::{GraphNode, NodeState},
        storage::Store,
    };

    use crate::services::scan::{
        collect_repo_files,
    };
    use super::super::common::{demo_project_summary, build_surface_state};

    #[test]
    fn typed_config_references_produce_graph_edges() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::create_dir_all(repo.join(".codex")).expect("codex dir");
        fs::write(
            repo.join(".codex").join("config.toml"),
            "[instructions]\ninclude = \"./policy.md\"\n",
        )
        .expect("config");
        fs::write(repo.join(".codex").join("policy.md"), "ok").expect("policy");

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
        let inventory = collect_repo_files(&repo, 5);
        let state = build_surface_state(
            &config,
            &store,
            &demo_project_summary(&repo, &home),
            &seed_catalog_map().expect("catalogs")["codex"],
            &inventory,
        )
        .expect("state");

        assert!(state
            .edges
            .iter()
            .any(|edge| edge.reason.contains("Typed config reference found")));
    }

    #[test]
    fn instruction_directive_references_become_effective_recursively() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::write(repo.join("AGENTS.md"), "Read CLAUDE.md\n").expect("agents");
        fs::write(repo.join("CLAUDE.md"), "@./nested.md\n").expect("claude");
        fs::write(repo.join("nested.md"), "ok").expect("nested");

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
        let inventory = collect_repo_files(&repo, 5);
        let state = build_surface_state(
            &config,
            &store,
            &demo_project_summary(&repo, &home),
            &seed_catalog_map().expect("catalogs")["codex"],
            &inventory,
        )
        .expect("state");

        let claude = state
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::Artifact(artifact) if artifact.display_path.ends_with("CLAUDE.md") => {
                    Some(artifact)
                }
                _ => None,
            })
            .expect("claude node");
        assert!(claude.states.contains(&NodeState::Effective));

        let nested = state
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::Artifact(artifact) if artifact.display_path.ends_with("nested.md") => {
                    Some(artifact)
                }
                _ => None,
            })
            .expect("nested node");
        assert!(nested.states.contains(&NodeState::Effective));
    }

    #[test]
    fn sentence_style_instruction_references_become_effective() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::write(repo.join("AGENTS.md"), "Read CLAUDE.md\n").expect("agents");
        fs::write(
            repo.join("CLAUDE.md"),
            "If prioritization is involved, read ANALYSIS.md and TODOS.md directly before planning.\n",
        )
        .expect("claude");
        fs::write(repo.join("ANALYSIS.md"), "ok").expect("analysis");
        fs::write(repo.join("TODOS.md"), "ok").expect("todos");

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
        let inventory = collect_repo_files(&repo, 5);
        let state = build_surface_state(
            &config,
            &store,
            &demo_project_summary(&repo, &home),
            &seed_catalog_map().expect("catalogs")["codex"],
            &inventory,
        )
        .expect("state");

        for expected in ["ANALYSIS.md", "TODOS.md"] {
            let node = state
                .nodes
                .iter()
                .find_map(|node| match node {
                    GraphNode::Artifact(artifact) if artifact.display_path.ends_with(expected) => {
                        Some(artifact)
                    }
                    _ => None,
                })
                .expect("referenced node");
            assert!(node.states.contains(&NodeState::Effective));
        }
    }

    #[test]
    fn docs_map_table_references_become_effective() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::create_dir_all(repo.join("docs").join("CODEMAPS")).expect("docs dir");
        fs::write(repo.join("AGENTS.md"), "Read CLAUDE.md\n").expect("agents");
        fs::write(
            repo.join("CLAUDE.md"),
            "## Docs Map\n\n| Need | Read |\n|---|---|\n| Conventions | `docs/CONTRIB.md` |\n| Architecture | `docs/CODEMAPS/architecture.md` |\n",
        )
        .expect("claude");
        fs::write(repo.join("docs").join("CONTRIB.md"), "ok").expect("contrib");
        fs::write(repo.join("docs").join("CODEMAPS").join("architecture.md"), "ok")
            .expect("architecture");

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
        let inventory = collect_repo_files(&repo, 5);
        let state = build_surface_state(
            &config,
            &store,
            &demo_project_summary(&repo, &home),
            &seed_catalog_map().expect("catalogs")["codex"],
            &inventory,
        )
        .expect("state");

        for expected in ["docs/CONTRIB.md", "docs/CODEMAPS/architecture.md"] {
            let node = state
                .nodes
                .iter()
                .find_map(|node| match node {
                    GraphNode::Artifact(artifact) if artifact.path.ends_with(expected) => {
                        Some(artifact)
                    }
                    _ => None,
                })
                .expect("docs-map node");
            assert!(node.states.contains(&NodeState::Effective));
        }
    }

    #[test]
    fn typed_config_reference_targets_become_effective() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::create_dir_all(repo.join(".codex")).expect("codex dir");
        fs::write(
            repo.join(".codex").join("config.toml"),
            "[instructions]\ninclude = \"./policy.md\"\n",
        )
        .expect("config");
        fs::write(repo.join(".codex").join("policy.md"), "ok").expect("policy");

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
        let inventory = collect_repo_files(&repo, 5);
        let state = build_surface_state(
            &config,
            &store,
            &demo_project_summary(&repo, &home),
            &seed_catalog_map().expect("catalogs")["codex"],
            &inventory,
        )
        .expect("state");

        let policy = state
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::Artifact(artifact) if artifact.display_path.ends_with("policy.md") => {
                    Some(artifact)
                }
                _ => None,
            })
            .expect("policy node");
        assert!(policy.states.contains(&NodeState::Effective));
        let verdict = state
            .verdicts
            .iter()
            .find(|verdict| verdict.entity_id == policy.id)
            .expect("policy verdict");
        assert!(verdict
            .why_included
            .iter()
            .any(|line| line.contains("Effective via")));
    }
}
