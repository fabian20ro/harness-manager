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
        reindex_project_tool_with_progress, scan_projects, collect_repo_files,
    };
    use crate::services::jobs::JobRegistry;
    use crate::services::graph::{
        build_surface_state_with_context, ScanRunContext,
    };
    use super::super::common::{demo_project_summary};

    #[test]
    fn scoped_reindex_refreshes_only_selected_surface_and_union_graph() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::write(repo.join("AGENTS.md"), "Initial.\n").expect("agents");

        let config = AppConfig {
            home_dir: home.clone(),
            store_root: temp.path().join("store"),
            default_roots: vec![home.join("git")],
            scan_max_depth: 5,
            known_global_dirs: vec![home.join(".codex"), home.join(".claude")],
            allowed_origins: vec!["http://127.0.0.1:4173".to_string()],
            allow_insecure_doc_hosts: false,
            max_snapshot_bytes: 5_000_000,
        };
        let store = Store::new(config.store_root.clone());
        let jobs = JobRegistry::new(store.clone());
        let projects = scan_projects(&config, &store, &jobs, None).expect("scan");
        let project = projects.first().expect("project");

        let before_index = store
            .read_json::<Vec<crate::domain::ProjectSummary>>(&store.projects_index_path())
            .expect("projects index");
        let before_project = before_index.first().expect("indexed project");
        let claude_state_before = fs::read(store.tool_state_path(&project.id, "claude_code"))
            .expect("claude state bytes");

        fs::create_dir_all(repo.join("docs")).expect("docs dir");
        fs::write(repo.join("AGENTS.md"), "@./docs/policy.md\n").expect("agents updated");
        fs::write(repo.join("docs").join("policy.md"), "ok\n").expect("policy");

        let mut progress = Vec::new();
        let state = reindex_project_tool_with_progress(
            &config,
            &store,
            &jobs,
            &project.id,
            "codex",
            |update| {
                progress.push(update);
                Ok(())
            },
        )
        .expect("reindex");

        assert_eq!(state.tool.id, "codex");
        assert!(progress.iter().any(|update| update.phase == "repo"));
        assert!(progress.iter().any(|update| update.phase == "walk"));
        assert!(progress.iter().any(|update| update.phase == "surface"));

        let after_index = store
            .read_json::<Vec<crate::domain::ProjectSummary>>(&store.projects_index_path())
            .expect("projects index");
        let after_project = after_index.first().expect("indexed project");
        assert!(after_project.indexed_at >= before_project.indexed_at);

        let inventory = store
            .read_json::<Vec<String>>(&store.inventory_path(&project.id))
            .expect("inventory");
        assert!(inventory.iter().any(|path| path == "docs/policy.md"));

        let claude_state_after = fs::read(store.tool_state_path(&project.id, "claude_code"))
            .expect("claude state bytes");
        assert_eq!(claude_state_after, claude_state_before);

        let graph_nodes = store
            .read_json::<Vec<crate::domain::GraphNode>>(&store.graph_nodes_path(&project.id))
            .expect("graph nodes");
        assert!(graph_nodes.iter().any(|node| matches!(
            node,
            GraphNode::Artifact(artifact) if artifact.path.ends_with("docs/policy.md")
        )));
    }

    #[test]
    fn codex_plugin_discovery_is_memoized_across_projects_and_keeps_project_local_enablement() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo_one = home.join("git").join("demo-one");
        let repo_two = home.join("git").join("demo-two");
        fs::create_dir_all(repo_one.join(".git")).expect("git dir");
        fs::create_dir_all(repo_two.join(".git")).expect("git dir");
        fs::create_dir_all(repo_one.join(".codex")).expect("codex dir");
        fs::create_dir_all(
            home.join(".codex")
                .join("plugins")
                .join("gmail")
                .join(".codex-plugin"),
        )
        .expect("plugin dir");
        fs::write(
            home.join(".codex")
                .join("plugins")
                .join("gmail")
                .join(".codex-plugin")
                .join("plugin.json"),
            r#"{"name":"gmail"}"#,
        )
        .expect("manifest");
        fs::write(
            repo_one.join(".codex").join("config.toml"),
            "[plugins.gmail]\nenabled = false\n",
        )
        .expect("repo config");

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
        let catalogs = seed_catalog_map().expect("catalogs");
        let inventory_one = collect_repo_files(&repo_one, 5);
        let inventory_two = collect_repo_files(&repo_two, 5);
        let mut scan_run = ScanRunContext::default();
        let mut progress = Vec::new();

        let state_one = build_surface_state_with_context(
            &config,
            &store,
            &demo_project_summary(&repo_one, &home),
            &catalogs["codex"],
            &inventory_one,
            &mut scan_run,
            &mut |update| {
                progress.push(update);
                Ok(())
            },
        )
        .expect("state one");
        let state_two = build_surface_state_with_context(
            &config,
            &store,
            &demo_project_summary(&repo_two, &home),
            &catalogs["codex"],
            &inventory_two,
            &mut scan_run,
            &mut |update| {
                progress.push(update);
                Ok(())
            },
        )
        .expect("state two");

        assert_eq!(scan_run.plugin_discovery_cache.call_counts.get("codex"), Some(&1));
        assert!(progress.iter().any(|update| {
            update
                .message
                .contains("Discovering Codex plugins for ~/git/demo-one")
        }));
        assert!(progress.iter().any(|update| {
            update
                .message
                .contains("Reusing cached Codex plugin discovery for ~/git/demo-two")
        }));

        let plugin_one = state_one
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::Plugin(plugin) if plugin.name == "gmail" => Some(plugin),
                _ => None,
            })
            .expect("plugin one");
        let plugin_two = state_two
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::Plugin(plugin) if plugin.name == "gmail" => Some(plugin),
                _ => None,
            })
            .expect("plugin two");

        assert!(plugin_one.states.contains(&NodeState::Inactive));
        assert!(plugin_two.states.contains(&NodeState::Effective));
    }

    #[test]
    fn claude_plugin_discovery_is_memoized_across_surfaces_in_one_run() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        let install_root = home
            .join(".claude")
            .join("plugins")
            .join("cache")
            .join("claude-plugins-official")
            .join("github")
            .join("1.0.0");
        fs::create_dir_all(install_root.join(".claude-plugin")).expect("plugin dir");
        fs::write(
            install_root.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"github"}"#,
        )
        .expect("manifest");
        fs::create_dir_all(home.join(".claude").join("plugins")).expect("plugins dir");
        fs::write(
            home.join(".claude").join("plugins").join("installed_plugins.json"),
            format!(
                r#"{{"version":2,"plugins":{{"github@claude-plugins-official":[{{"installPath":"{}"}}]}}}}"#,
                install_root.display()
            ),
        )
        .expect("installed index");

        let config = AppConfig {
            home_dir: home.clone(),
            store_root: temp.path().join("store"),
            default_roots: vec![home.join("git")],
            scan_max_depth: 5,
            known_global_dirs: vec![home.join(".claude")],
            allowed_origins: vec!["http://127.0.0.1:4173".to_string()],
            allow_insecure_doc_hosts: false,
            max_snapshot_bytes: 5_000_000,
        };
        let store = Store::new(config.store_root.clone());
        let catalogs = seed_catalog_map().expect("catalogs");
        let inventory = collect_repo_files(&repo, 5);
        let mut scan_run = ScanRunContext::default();
        let mut progress = Vec::new();

        build_surface_state_with_context(
            &config,
            &store,
            &demo_project_summary(&repo, &home),
            &catalogs["claude_code"],
            &inventory,
            &mut scan_run,
            &mut |update| {
                progress.push(update);
                Ok(())
            },
        )
        .expect("claude code state");
        build_surface_state_with_context(
            &config,
            &store,
            &demo_project_summary(&repo, &home),
            &catalogs["claude_cowork"],
            &inventory,
            &mut scan_run,
            &mut |update| {
                progress.push(update);
                Ok(())
            },
        )
        .expect("claude cowork state");

        assert_eq!(scan_run.plugin_discovery_cache.call_counts.get("claude"), Some(&1));
        assert!(progress.iter().any(|update| {
            update
                .message
                .contains("Discovering Claude plugins for ~/git/demo")
        }));
        assert!(progress.iter().any(|update| {
            update
                .message
                .contains("Reusing cached Claude plugin discovery for ~/git/demo")
        }));
    }
}
