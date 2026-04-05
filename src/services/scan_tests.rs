#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path};

    use chrono::Utc;
    use tempfile::TempDir;
    use anyhow::Result;

    use crate::{
        catalogs::seed_catalog_map,
        config::AppConfig,
        domain::{GraphNode, NodeState, ProjectKind, ProjectSummary, SurfaceState, ToolCatalog, EdgeType},
        storage::Store,
    };

    use crate::services::scan::{
        reindex_project_tool_with_progress, scan_projects, scan_projects_with_progress,
        collect_repo_files,
    };
    use crate::services::graph::{
        build_surface_state_with_context, stable_id, ScanRunContext,
    };
    use crate::services::projects::discovery::display_path;

    fn build_surface_state(
        config: &AppConfig,
        store: &Store,
        project: &ProjectSummary,
        catalog: &ToolCatalog,
        inventory: &[String],
    ) -> Result<SurfaceState> {
        let mut scan_run = ScanRunContext::default();
        build_surface_state_with_context(
            config,
            store,
            project,
            catalog,
            inventory,
            &mut scan_run,
            &mut |_| Ok(()),
        )
    }

    fn demo_project_summary(root: &Path, home: &Path) -> ProjectSummary {
        ProjectSummary {
            id: "demo".to_string(),
            root_path: root.to_string_lossy().to_string(),
            display_path: display_path(root, home),
            name: "demo".to_string(),
            kind: ProjectKind::GitRepo,
            discovery_reason: String::new(),
            signal_score: 300,
            indexed_at: Utc::now(),
            status: "ready".to_string(),
        }
    }

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
        let projects = scan_projects(&config, &store, None).expect("scan");
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
        let mut progress = Vec::new();

        let projects = scan_projects_with_progress(&config, &store, None, |update| {
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

        let projects = scan_projects(&config, &store, None).expect("scan");
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

        let projects = scan_projects(&config, &store, None).expect("scan");
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

        let projects = scan_projects(&config, &store, None).expect("scan");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].kind, ProjectKind::PluginPackage);
        assert_eq!(projects[0].root_path, skill_root.to_string_lossy());
        assert!(projects[0]
            .discovery_reason
            .contains(".github/skills/*/SKILL.md"));
    }

    #[test]
    fn scan_discovers_plugin_packages_from_plugin_signals() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let plugin_root = home
            .join(".codex")
            .join(".tmp")
            .join("plugins")
            .join("plugins")
            .join("vercel");
        fs::create_dir_all(plugin_root.join("skills").join("nextjs")).expect("skills dir");
        fs::write(
            plugin_root.join("skills").join("nextjs").join("SKILL.md"),
            "---\nname: Next.js\ndescription: test\n---\n",
        )
        .expect("skill");
        fs::write(plugin_root.join(".mcp.json"), "{}").expect("mcp");

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

        let projects = scan_projects(&config, &store, None).expect("scan");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].kind, ProjectKind::PluginPackage);
        assert_eq!(projects[0].root_path, plugin_root.to_string_lossy());
        assert!(projects[0].discovery_reason.contains(".mcp.json"));
    }

    #[test]
    fn scan_merges_duplicate_workspace_signals_for_same_root() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let workspace = home.join("scratch").join("shared-signals");
        fs::create_dir_all(workspace.join(".codex")).expect("codex dir");
        fs::write(workspace.join("AGENTS.md"), "Use local policy.\n").expect("agents");
        fs::write(workspace.join(".codex").join("config.toml"), "model = \"gpt-5\"\n")
            .expect("config");

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

        let projects = scan_projects(&config, &store, None).expect("scan");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].kind, ProjectKind::WorkspaceCandidate);
        assert_eq!(projects[0].root_path, workspace.to_string_lossy());
        assert!(projects[0].discovery_reason.contains("AGENTS.md"));
        assert_eq!(projects[0].signal_score, 220);
    }

    #[test]
    fn git_roots_outrank_nested_plugin_packages() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        let nested_plugin = repo.join("plugins").join("bundle");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::create_dir_all(nested_plugin.join("skills").join("nextjs")).expect("skills dir");
        fs::write(
            nested_plugin.join("skills").join("nextjs").join("SKILL.md"),
            "---\nname: Next.js\ndescription: test\n---\n",
        )
        .expect("skill");

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

        let projects = scan_projects(&config, &store, None).expect("scan");
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].kind, ProjectKind::GitRepo);
        assert_eq!(projects[0].root_path, repo.to_string_lossy());
    }

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
        let projects = scan_projects(&config, &store, None).expect("scan");
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
            &project.id,
            "codex",
            |update| {
                progress.push(update);
                Ok(())
            },
        )
        .expect("scoped reindex");

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

    #[test]
    fn codex_plugin_disabled_from_config() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
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
            home.join(".codex").join("config.toml"),
            "[plugins.gmail]\nenabled = false\n",
        )
        .expect("config");

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

        let plugin = state
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::Plugin(plugin) if plugin.name == "gmail" => Some(plugin),
                _ => None,
            })
            .expect("plugin node");
        assert!(plugin.states.contains(&NodeState::Inactive));
    }

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

    #[test]
    fn codex_plugins_discovered_from_cache_layout() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        let plugin_root = home
            .join(".codex")
            .join(".tmp")
            .join("plugins")
            .join("plugins")
            .join("github");
        fs::create_dir_all(plugin_root.join(".codex-plugin")).expect("plugin dir");
        fs::write(
            plugin_root.join(".codex-plugin").join("plugin.json"),
            r#"{"name":"github"}"#,
        )
        .expect("manifest");
        fs::write(
            home.join(".codex").join("config.toml"),
            "[plugins.\"github@openai-curated\"]\nenabled = true\n",
        )
        .expect("config");

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

        let plugin = state
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::Plugin(plugin) if plugin.name == "github" => Some(plugin),
                _ => None,
            })
            .expect("plugin node");
        assert_eq!(plugin.install_root, plugin_root.to_string_lossy());
        assert!(plugin.discovery_sources.iter().any(|source| source == "cache_layout"));
    }

    #[test]
    fn codex_plugin_manifest_refs_resolve_from_plugin_root_in_surface_state() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        let plugin_root = home
            .join(".codex")
            .join(".tmp")
            .join("plugins")
            .join("plugins")
            .join("vercel");
        fs::create_dir_all(plugin_root.join(".codex-plugin")).expect("plugin dir");
        fs::create_dir_all(plugin_root.join("skills").join("skill")).expect("skills dir");
        fs::write(
            plugin_root.join(".codex-plugin").join("plugin.json"),
            r#"{"name":"vercel","skills":"./skills/skill/SKILL.md"}"#,
        )
        .expect("manifest");
        fs::write(
            plugin_root.join("skills").join("skill").join("SKILL.md"),
            "---\nname: Example Skill\ndescription: Example description\n---\n",
        )
        .expect("skill");

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

        assert!(state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::PluginArtifact(artifact)
                if artifact.path
                    == plugin_root
                        .join("skills")
                        .join("skill")
                        .join("SKILL.md")
                        .to_string_lossy()
        )));
        assert!(!state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::Artifact(artifact)
                if artifact.path.contains("/.codex-plugin/skills/")
                    || artifact.path.contains("\\.codex-plugin\\skills\\")
        )));
    }

    #[test]
    fn codex_directory_references_link_to_existing_skill_artifacts_without_directory_blowup() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        let plugin_root = home
            .join(".codex")
            .join(".tmp")
            .join("plugins")
            .join("plugins")
            .join("vercel");
        fs::create_dir_all(plugin_root.join(".codex-plugin")).expect("plugin dir");
        fs::create_dir_all(plugin_root.join("skills").join("root")).expect("skills dir");
        fs::create_dir_all(plugin_root.join("skills").join("nested").join("deep"))
            .expect("nested skills dir");
        fs::write(
            plugin_root.join(".codex-plugin").join("plugin.json"),
            r#"{"name":"vercel","skills":"./skills/","mcpServers":"./.mcp.json","hooks":"./hooks.json"}"#,
        )
        .expect("manifest");
        fs::write(
            plugin_root.join("skills").join("root").join("SKILL.md"),
            "---\nname: Root Skill\ndescription: Root description\n---\n",
        )
        .expect("root skill");
        fs::write(
            plugin_root.join("skills").join("root").join("notes.md"),
            "supporting notes\n",
        )
        .expect("notes");
        fs::write(
            plugin_root
                .join("skills")
                .join("nested")
                .join("deep")
                .join("SKILL.md"),
            "---\nname: Deep Skill\ndescription: Deep description\n---\n",
        )
        .expect("deep skill");
        fs::write(plugin_root.join(".mcp.json"), "{}").expect("mcp");
        fs::write(plugin_root.join("hooks.json"), "{}").expect("hooks");

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

        let skills_dir = plugin_root.join("skills").to_string_lossy().to_string();
        let root_skill = plugin_root
            .join("skills")
            .join("root")
            .join("SKILL.md")
            .to_string_lossy()
            .to_string();
        let nested_skill = plugin_root
            .join("skills")
            .join("nested")
            .join("deep")
            .join("SKILL.md")
            .to_string_lossy()
            .to_string();
        let mcp_file = plugin_root.join(".mcp.json").to_string_lossy().to_string();
        let hooks_file = plugin_root.join("hooks.json").to_string_lossy().to_string();

        assert!(state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::Artifact(artifact) if artifact.path == skills_dir
        )));
        assert!(state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::PluginArtifact(artifact)
                if artifact.path == root_skill && artifact.name.as_deref() == Some("Root Skill")
        )));
        assert!(state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::PluginArtifact(artifact)
                if artifact.path == nested_skill && artifact.name.as_deref() == Some("Deep Skill")
        )));
        assert!(state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::Artifact(artifact) if artifact.path == mcp_file
        )));
        assert!(state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::Artifact(artifact) if artifact.path == hooks_file
        )));
        assert!(!state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::Artifact(artifact)
                if artifact.path
                    == plugin_root
                        .join("skills")
                        .join("root")
                        .join("notes.md")
                        .to_string_lossy()
        )));

        let root_skill_id = stable_id("plugin_artifact", &root_skill);
        let nested_skill_id = stable_id("plugin_artifact", &nested_skill);
        let skills_dir_id = stable_id("reference", &skills_dir);
        assert!(state.edges.iter().any(|edge| {
            edge.from == skills_dir_id && edge.to == root_skill_id && matches!(edge.edge_type, EdgeType::References)
        }));
        assert!(state.edges.iter().any(|edge| {
            edge.from == skills_dir_id && edge.to == nested_skill_id && matches!(edge.edge_type, EdgeType::References)
        }));
        assert!(state.verdicts.iter().any(|verdict| {
            verdict.entity_id == root_skill_id && verdict.states.contains(&NodeState::Effective)
        }));
        assert!(state.verdicts.iter().any(|verdict| {
            verdict.entity_id == nested_skill_id && verdict.states.contains(&NodeState::Effective)
        }));
    }

    #[test]
    fn codex_skill_metadata_is_parsed_from_frontmatter_and_openai_yaml() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        let plugin_root = home
            .join(".codex")
            .join(".tmp")
            .join("plugins")
            .join("plugins")
            .join("vercel");
        fs::create_dir_all(plugin_root.join(".codex-plugin")).expect("plugin dir");
        fs::create_dir_all(plugin_root.join("skills").join("nextjs").join("agents"))
            .expect("agents dir");
        fs::write(
            plugin_root.join(".codex-plugin").join("plugin.json"),
            r#"{"name":"vercel","skills":"./skills/"}"#,
        )
        .expect("manifest");
        fs::write(
            plugin_root.join("skills").join("nextjs").join("SKILL.md"),
            "---\nname: Next.js App Router\ndescription: Build and debug App Router projects\nretrieval:\n  aliases: [nextjs, app-router]\nintents: [routing, caching]\n---\n# body\n",
        )
        .expect("skill");
        fs::write(
            plugin_root
                .join("skills")
                .join("nextjs")
                .join("agents")
                .join("openai.yaml"),
            "displayName: Next.js\ninvocation:\n  when: manual\n",
        )
        .expect("openai yaml");

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

        let skill = state
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::PluginArtifact(artifact)
                    if artifact.path
                        == plugin_root
                            .join("skills")
                            .join("nextjs")
                            .join("SKILL.md")
                            .to_string_lossy() => Some(artifact),
                _ => None,
            })
            .expect("skill node");

        assert_eq!(skill.name.as_deref(), Some("Next.js App Router"));
        assert_eq!(
            skill.description.as_deref(),
            Some("Build and debug App Router projects")
        );
        let metadata = skill.metadata.as_ref().expect("skill metadata");
        assert_eq!(metadata["openai"]["displayName"], "Next.js");
        assert_eq!(
            metadata["legacy_frontmatter"]["retrieval"]["aliases"][0],
            "nextjs"
        );
        assert_eq!(
            metadata["legacy_frontmatter"]["intents"][1],
            "caching"
        );
    }

    #[test]
    fn codex_missing_declared_skill_paths_are_broken_plugin_artifacts() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        let plugin_root = home
            .join(".codex")
            .join(".tmp")
            .join("plugins")
            .join("plugins")
            .join("vercel");
        fs::create_dir_all(plugin_root.join(".codex-plugin")).expect("plugin dir");
        fs::write(
            plugin_root.join(".codex-plugin").join("plugin.json"),
            r#"{"name":"vercel","skills":"./skills/missing/SKILL.md"}"#,
        )
        .expect("manifest");

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

        let missing_skill = state
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::PluginArtifact(artifact)
                    if artifact.path.ends_with("skills/missing/SKILL.md") => Some(artifact),
                _ => None,
            })
            .expect("missing skill node");
        assert!(missing_skill.states.contains(&NodeState::BrokenReference));
        assert!(missing_skill.states.contains(&NodeState::Unresolved));
    }

    #[test]
    fn claude_plugins_discovered_from_installed_index() {
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
        let inventory = collect_repo_files(&repo, 5);
        let state = build_surface_state(
            &config,
            &store,
            &demo_project_summary(&repo, &home),
            &seed_catalog_map().expect("catalogs")["claude_code"],
            &inventory,
        )
        .expect("state");

        let plugin = state
            .nodes
            .iter()
            .find_map(|node| match node {
                GraphNode::Plugin(plugin) if plugin.name == "github" => Some(plugin),
                _ => None,
            })
            .expect("plugin node");
        assert_eq!(plugin.install_root, install_root.to_string_lossy());
        assert!(plugin
            .discovery_sources
            .iter()
            .any(|source| source == "install_index"));
    }

    #[test]
    fn claude_plugin_manifest_refs_resolve_from_plugin_root_in_surface_state() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        let plugin_root = home
            .join(".claude")
            .join("plugins")
            .join("cache")
            .join("market")
            .join("vercel")
            .join("1.0.0");
        fs::create_dir_all(plugin_root.join(".claude-plugin")).expect("plugin dir");
        fs::create_dir_all(plugin_root.join("agents")).expect("agents dir");
        fs::write(
            plugin_root.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"vercel","agents":["./agents/architect.md"]}"#,
        )
        .expect("manifest");
        fs::write(plugin_root.join("agents").join("architect.md"), "ok").expect("agent");

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
        let inventory = collect_repo_files(&repo, 5);
        let state = build_surface_state(
            &config,
            &store,
            &demo_project_summary(&repo, &home),
            &seed_catalog_map().expect("catalogs")["claude_code"],
            &inventory,
        )
        .expect("state");

        assert!(state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::Artifact(artifact)
                if artifact.path == plugin_root.join("agents").join("architect.md").to_string_lossy()
        )));
        assert!(!state.nodes.iter().any(|node| matches!(
            node,
            GraphNode::Artifact(artifact)
                if artifact.path.contains("/.claude-plugin/agents/")
                    || artifact.path.contains("\\.claude-plugin\\agents\\")
        )));
    }
}
