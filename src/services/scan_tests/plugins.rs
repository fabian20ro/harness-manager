#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::TempDir;

    use crate::{
        catalogs::seed_catalog_map,
        config::AppConfig,
        domain::{GraphNode, NodeState, EdgeType},
        storage::Store,
    };

    use crate::services::scan::{
        collect_repo_files,
    };
    use super::super::common::{demo_project_summary, build_surface_state};

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
        fs::create_dir_all(repo.join(".codex")).expect("codex dir");
        fs::write(
            repo.join(".codex").join("config.toml"),
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

        let root_skill_id = crate::services::graph::stable_id("plugin_artifact", &root_skill);
        let nested_skill_id = crate::services::graph::stable_id("plugin_artifact", &nested_skill);
        let skills_dir_id = crate::services::graph::stable_id("reference", &skills_dir);
        assert!(state.edges.iter().any(|edge| {
            edge.from == skills_dir_id && edge.to == root_skill_id && matches!(edge.edge_type, EdgeType::References)
        }));
        assert!(state.edges.iter().any(|edge| {
            edge.from == skills_dir_id && edge.to == nested_skill_id && matches!(edge.edge_type, EdgeType::References)
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
