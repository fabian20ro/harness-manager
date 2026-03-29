use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSetBuilder};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{
    catalogs::{catalog_path, seed_catalogs},
    config::AppConfig,
    domain::{
        ArtifactNode, ArtifactRule, ArtifactType, EdgeType, GraphEdge, GraphNode, NodeState,
        PluginArtifactNode, PluginNode, ProjectNode, ProjectSummary, RemoteSnapshotNode, ScopeType,
        SnapshotAssociation, SurfaceState, ToolCatalog, ToolContext, ToolContextNode, Verdict,
    },
    services::refs::{extract_references, ResolverContext},
    storage::Store,
};

#[derive(Clone, Debug)]
struct PromotableEdge {
    from: String,
    to: String,
    reason: String,
}

#[derive(Default)]
struct ReferenceCollection {
    edges: Vec<GraphEdge>,
    verdicts: Vec<Verdict>,
    promotable_edges: Vec<PromotableEdge>,
}

#[derive(Clone, Debug)]
struct PluginCandidate {
    key: String,
    name: String,
    install_root: PathBuf,
    manifest_path: Option<PathBuf>,
    readme_path: Option<PathBuf>,
    discovery_sources: Vec<String>,
    disabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanProgress {
    pub phase: String,
    pub message: String,
    pub current_path: Option<String>,
    pub items_done: Option<usize>,
    pub items_total: Option<usize>,
}

pub fn refresh_catalogs(
    store: &Store,
    supplied_catalogs: Option<Vec<ToolCatalog>>,
) -> Result<Vec<ToolCatalog>> {
    let catalogs = supplied_catalogs.unwrap_or(seed_catalogs()?);
    for catalog in &catalogs {
        let path = catalog_path(&store.root, &catalog.surface, &catalog.version);
        store.write_json(&path, catalog)?;
    }
    Ok(catalogs)
}

pub fn load_catalogs(store: &Store) -> Result<HashMap<String, ToolCatalog>> {
    let seeds = seed_catalogs()?;
    let mut map = HashMap::new();
    for seed in seeds {
        let path = catalog_path(&store.root, &seed.surface, &seed.version);
        let catalog = if path.exists() {
            store.read_json(&path)?
        } else {
            store.write_json(&path, &seed)?;
            seed
        };
        map.insert(catalog.surface.clone(), catalog);
    }
    Ok(map)
}

pub fn scan_projects(
    config: &AppConfig,
    store: &Store,
    roots: Option<Vec<String>>,
) -> Result<Vec<ProjectSummary>> {
    scan_projects_with_progress(config, store, roots, |_| Ok(()))
}

pub fn scan_projects_with_progress<F>(
    config: &AppConfig,
    store: &Store,
    roots: Option<Vec<String>>,
    mut on_progress: F,
) -> Result<Vec<ProjectSummary>>
where
    F: FnMut(ScanProgress) -> Result<()>,
{
    store.ensure_layout()?;
    let catalogs = load_catalogs(store)?;
    let roots = roots
        .unwrap_or_else(|| {
            config
                .default_roots
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect()
        })
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let repo_roots = discover_repos_with_progress(
        &roots,
        config.scan_max_depth,
        &config.home_dir,
        &mut on_progress,
    )?;

    let mut summaries = Vec::new();
    let total_repos = repo_roots.len();
    for (index, repo_root) in repo_roots.iter().enumerate() {
        let repo_display_path = display_path(repo_root, &config.home_dir);
        on_progress(ScanProgress {
            phase: "repo".to_string(),
            message: format!("Indexing repo {repo_display_path}"),
            current_path: Some(repo_display_path.clone()),
            items_done: Some(index),
            items_total: Some(total_repos),
        })?;
        let summary = scan_project(
            config,
            store,
            &catalogs,
            repo_root,
            index + 1,
            total_repos,
            &mut on_progress,
        )?;
        summaries.push(summary);
    }
    store.write_json(&store.projects_index_path(), &summaries)?;
    Ok(summaries)
}

pub fn load_surface_state(store: &Store, project_id: &str, tool: &str) -> Result<SurfaceState> {
    store.read_json(&store.tool_state_path(project_id, tool))
}

pub fn rebuild_surface_state(
    config: &AppConfig,
    store: &Store,
    project_id: &str,
    tool: &str,
) -> Result<SurfaceState> {
    let projects = store
        .maybe_read_json::<Vec<ProjectSummary>>(&store.projects_index_path())?
        .unwrap_or_default();
    let project = projects
        .into_iter()
        .find(|project| project.id == project_id)
        .context("project not found in index")?;
    let inventory = store.read_json::<Vec<String>>(&store.inventory_path(project_id))?;
    let catalogs = load_catalogs(store)?;
    let catalog = catalogs.get(tool).context("tool catalog not found")?;
    let state = build_surface_state(config, store, &project, catalog, &inventory)?;
    store.write_json(&store.tool_state_path(project_id, tool), &state)?;
    Ok(state)
}

fn scan_project(
    config: &AppConfig,
    store: &Store,
    catalogs: &HashMap<String, ToolCatalog>,
    repo_root: &Path,
    repo_index: usize,
    total_repos: usize,
    on_progress: &mut dyn FnMut(ScanProgress) -> Result<()>,
) -> Result<ProjectSummary> {
    let indexed_at = Utc::now();
    let project_id = stable_id("project", &repo_root.to_string_lossy());
    let summary = ProjectSummary {
        id: project_id.clone(),
        root_path: repo_root.to_string_lossy().to_string(),
        display_path: display_path(repo_root, &config.home_dir),
        name: repo_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("repo")
            .to_string(),
        indexed_at,
        status: "ready".to_string(),
    };

    let project_dir = store.project_dir(&project_id);
    fs::create_dir_all(project_dir.join("tool-state"))?;
    let repo_display_path = display_path(repo_root, &config.home_dir);
    let inventory = collect_repo_files_with_progress(
        repo_root,
        config.scan_max_depth,
        &config.home_dir,
        &mut |current_dir| {
            on_progress(ScanProgress {
                phase: "walk".to_string(),
                message: format!("Scanning {}", current_dir),
                current_path: Some(current_dir),
                items_done: Some(repo_index),
                items_total: Some(total_repos),
            })
        },
    )?;
    store.write_json(&store.inventory_path(&project_id), &inventory)?;

    let mut union_nodes = BTreeMap::<String, GraphNode>::new();
    let mut union_edges = Vec::<GraphEdge>::new();

    let total_surfaces = catalogs.len();
    for (surface_index, catalog) in catalogs.values().enumerate() {
        on_progress(ScanProgress {
            phase: "surface".to_string(),
            message: format!(
                "Evaluating {} for {}",
                catalog.display_name, repo_display_path
            ),
            current_path: Some(repo_display_path.clone()),
            items_done: Some(surface_index + 1),
            items_total: Some(total_surfaces),
        })?;
        let state = build_surface_state(config, store, &summary, catalog, &inventory)?;
        for node in &state.nodes {
            union_nodes.insert(node.id().to_string(), node.clone());
        }
        union_edges.extend(state.edges.clone());
        store.write_json(
            &store.tool_state_path(&project_id, &catalog.surface),
            &state,
        )?;
    }

    let deduped_edges = dedupe_edges(union_edges);
    store.write_json(
        &store.graph_nodes_path(&project_id),
        &union_nodes.into_values().collect::<Vec<_>>(),
    )?;
    store.write_json(&store.graph_edges_path(&project_id), &deduped_edges)?;

    Ok(summary)
}

fn build_surface_state(
    config: &AppConfig,
    store: &Store,
    project: &ProjectSummary,
    catalog: &ToolCatalog,
    inventory: &[String],
) -> Result<SurfaceState> {
    let mut nodes = BTreeMap::<String, GraphNode>::new();
    let mut edges = Vec::<GraphEdge>::new();
    let mut verdicts = Vec::<Verdict>::new();
    let indexed_at = Utc::now();

    let project_node = GraphNode::Project(ProjectNode {
        id: format!("project:{}", project.id),
        name: project.name.clone(),
        root_path: project.root_path.clone(),
        display_path: project.display_path.clone(),
    });
    let tool_ctx = ToolContext {
        id: catalog.surface.clone(),
        family: catalog.family.clone(),
        display_name: catalog.display_name.clone(),
        catalog_version: catalog.version.clone(),
        support_level: catalog.support_level.clone(),
    };
    let tool_node = GraphNode::ToolContext(ToolContextNode {
        id: format!("tool:{}", catalog.surface),
        tool: tool_ctx.clone(),
    });
    nodes.insert(project_node.id().to_string(), project_node.clone());
    nodes.insert(tool_node.id().to_string(), tool_node.clone());
    edges.push(GraphEdge {
        from: project_node.id().to_string(),
        to: tool_node.id().to_string(),
        edge_type: EdgeType::AppliesTo,
        hardness: "hard".to_string(),
        reason: format!("{} selected for project inspection.", catalog.display_name),
    });

    let repo_root = PathBuf::from(&project.root_path);
    let mut basename_to_node = HashMap::<String, String>::new();

    for artifact in
        collect_artifacts_from_rules(&repo_root, inventory, catalog, &config.home_dir, indexed_at)?
    {
        basename_to_node.insert(
            Path::new(&artifact.path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&artifact.path)
                .to_string(),
            artifact.id.clone(),
        );
        edges.push(GraphEdge {
            from: tool_node.id().to_string(),
            to: artifact.id.clone(),
            edge_type: EdgeType::Activates,
            hardness: "hard".to_string(),
            reason: artifact.reason.clone(),
        });
        verdicts.push(node_verdict(
            &artifact.id,
            &artifact.states,
            &artifact.reason,
        ));
        nodes.insert(artifact.id.clone(), GraphNode::Artifact(artifact));
    }

    for artifact in collect_global_locations(config, catalog, indexed_at)? {
        let basename = Path::new(&artifact.path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&artifact.path)
            .to_string();
        let mut artifact = artifact;
        if let Some(repo_artifact_id) = basename_to_node.get(&basename) {
            if !artifact.states.contains(&NodeState::Shadowed) {
                artifact.states.push(NodeState::Shadowed);
            }
            artifact.reason = format!(
                "{} Repo-local artifact with same basename shadows global path.",
                artifact.reason
            );
            verdicts.push(Verdict {
                entity_id: artifact.id.clone(),
                states: artifact.states.clone(),
                why_included: vec![format!(
                    "Detected in global scope: {}",
                    artifact.display_path
                )],
                why_excluded: vec!["Shadowed by repo-local artifact.".to_string()],
                shadowed_by: vec![repo_artifact_id.clone()],
                provenance_paths: vec![vec![tool_node.id().to_string(), artifact.id.clone()]],
            });
            edges.push(GraphEdge {
                from: repo_artifact_id.clone(),
                to: artifact.id.clone(),
                edge_type: EdgeType::Shadows,
                hardness: "soft".to_string(),
                reason: "Repo-local artifact shadows global artifact.".to_string(),
            });
        } else {
            verdicts.push(node_verdict(
                &artifact.id,
                &artifact.states,
                &artifact.reason,
            ));
        }
        edges.push(GraphEdge {
            from: tool_node.id().to_string(),
            to: artifact.id.clone(),
            edge_type: EdgeType::Loads,
            hardness: "hard".to_string(),
            reason: artifact.reason.clone(),
        });
        nodes.insert(artifact.id.clone(), GraphNode::Artifact(artifact));
    }

    if let Some(plugin_system) = &catalog.plugin_system {
        let (plugin_nodes, plugin_edges, plugin_verdicts) = collect_plugins(
            config,
            catalog,
            plugin_system,
            &tool_node,
            &repo_root,
            indexed_at,
        )?;
        for node in plugin_nodes {
            nodes.insert(node.id().to_string(), node);
        }
        edges.extend(plugin_edges);
        verdicts.extend(plugin_verdicts);
    }

    let snapshot_path = store
        .project_dir(&project.id)
        .join(format!("remote-snapshot-{}.json", catalog.surface));
    if let Some(snapshot_assoc) = store.maybe_read_json::<SnapshotAssociation>(&snapshot_path)? {
        let node = GraphNode::RemoteSnapshot(RemoteSnapshotNode {
            id: format!("snapshot:{}", snapshot_assoc.snapshot.id),
            url: snapshot_assoc.snapshot.url,
            fetched_at: snapshot_assoc.snapshot.fetched_at,
            content_path: snapshot_assoc.snapshot.content_path,
            normalized_hash: snapshot_assoc.snapshot.normalized_hash,
            linked_urls: snapshot_assoc.snapshot.linked_urls,
        });
        edges.push(GraphEdge {
            from: tool_node.id().to_string(),
            to: node.id().to_string(),
            edge_type: EdgeType::FetchedFrom,
            hardness: "hard".to_string(),
            reason: "Fetched docs associated to this tool context.".to_string(),
        });
        nodes.insert(node.id().to_string(), node);
    }

    let reference_collection =
        collect_reference_edges(&mut nodes, &tool_node, config, indexed_at)?;
    edges.extend(reference_collection.edges);
    verdicts.extend(reference_collection.verdicts);
    promote_effective_closure(&mut nodes, &edges, &mut verdicts, &reference_collection.promotable_edges);

    Ok(SurfaceState {
        project: project.clone(),
        tool: tool_ctx,
        nodes: nodes.into_values().collect(),
        edges: dedupe_edges(edges),
        verdicts,
        last_indexed_at: indexed_at,
    })
}

fn collect_artifacts_from_rules(
    repo_root: &Path,
    inventory: &[String],
    catalog: &ToolCatalog,
    home_dir: &Path,
    indexed_at: DateTime<Utc>,
) -> Result<Vec<ArtifactNode>> {
    let mut nodes = Vec::new();
    for ArtifactRule {
        glob,
        artifact_type,
        reason,
        states,
    } in &catalog.artifact_rules
    {
        let mut builder = GlobSetBuilder::new();
        builder.add(Glob::new(glob)?);
        let matcher = builder.build()?;

        for relative in inventory {
            if matcher.is_match(relative) {
                let full_path = repo_root.join(relative);
                let metadata = fs::metadata(&full_path).ok();
                let scope_type = if relative.contains('/') {
                    ScopeType::Subdirectory
                } else {
                    ScopeType::Repo
                };
                nodes.push(ArtifactNode {
                    id: stable_id(&catalog.surface, &full_path.to_string_lossy()),
                    path: full_path.to_string_lossy().to_string(),
                    display_path: display_path(&full_path, home_dir),
                    artifact_type: artifact_type.clone(),
                    tool_family: catalog.family.clone(),
                    scope_type,
                    states: states.clone(),
                    confidence: confidence_from_states(states),
                    origin: "repo".to_string(),
                    last_indexed_at: indexed_at,
                    hash: file_hash(&full_path).unwrap_or_else(|_| "missing".to_string()),
                    mtime: metadata.and_then(mtime_utc),
                    reason: reason.clone(),
                });
            }
        }
    }
    Ok(nodes)
}

fn collect_global_locations(
    config: &AppConfig,
    catalog: &ToolCatalog,
    indexed_at: DateTime<Utc>,
) -> Result<Vec<ArtifactNode>> {
    let mut nodes = Vec::new();
    for location in &catalog.known_locations {
        let path = resolve_catalog_path(&location.path, &config.home_dir, None);
        if !path.exists() {
            continue;
        }
        let metadata = fs::metadata(&path).ok();
        nodes.push(ArtifactNode {
            id: stable_id(&catalog.surface, &path.to_string_lossy()),
            path: path.to_string_lossy().to_string(),
            display_path: display_path(&path, &config.home_dir),
            artifact_type: location.artifact_type.clone(),
            tool_family: catalog.family.clone(),
            scope_type: location.scope_type.clone(),
            states: location.states.clone(),
            confidence: confidence_from_states(&location.states),
            origin: "global".to_string(),
            last_indexed_at: indexed_at,
            hash: file_hash(&path).unwrap_or_else(|_| "directory".to_string()),
            mtime: metadata.and_then(mtime_utc),
            reason: location.reason.clone(),
        });
    }
    Ok(nodes)
}

fn collect_plugins(
    config: &AppConfig,
    catalog: &ToolCatalog,
    plugin_system: &crate::domain::PluginSystemCatalog,
    tool_node: &GraphNode,
    repo_root: &Path,
    _indexed_at: DateTime<Utc>,
) -> Result<(Vec<GraphNode>, Vec<GraphEdge>, Vec<Verdict>)> {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut verdicts = Vec::new();
    let config_paths = plugin_system
        .config_paths
        .iter()
        .map(|path| resolve_catalog_path(path, &config.home_dir, Some(repo_root)))
        .collect::<Vec<_>>();
    let candidates = discover_plugins(config, plugin_system, repo_root, &config_paths)?;
    for candidate in candidates {
        let mut states = vec![NodeState::Installed, NodeState::Configured];
        let reason = if candidate.disabled {
            states.push(NodeState::Inactive);
            format!(
                "Plugin discovered via {} but disabled in config.",
                candidate.discovery_sources.join(", ")
            )
        } else {
            states.push(NodeState::Effective);
            format!(
                "Plugin discovered via {} and not disabled in config.",
                candidate.discovery_sources.join(", ")
            )
        };
        let plugin_id = stable_id("plugin", &format!("{}:{}", plugin_system.system, candidate.key));
        let plugin_node = GraphNode::Plugin(PluginNode {
            id: plugin_id.clone(),
            name: candidate.name.clone(),
            plugin_system: plugin_system.system.clone(),
            install_root: candidate.install_root.to_string_lossy().to_string(),
            display_path: display_path(&candidate.install_root, &config.home_dir),
            manifest_path: candidate
                .manifest_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            discovery_sources: candidate.discovery_sources.clone(),
            states: states.clone(),
            confidence: if candidate.disabled { 0.88 } else { 0.94 },
            reason: reason.clone(),
        });
        verdicts.push(node_verdict(&plugin_id, &states, &reason));
        edges.push(GraphEdge {
            from: plugin_id.clone(),
            to: tool_node.id().to_string(),
            edge_type: EdgeType::InstalledIn,
            hardness: "hard".to_string(),
            reason: format!("Plugin belongs to {} plugin system.", catalog.display_name),
        });
        edges.push(GraphEdge {
            from: plugin_id.clone(),
            to: tool_node.id().to_string(),
            edge_type: if candidate.disabled {
                EdgeType::Disables
            } else {
                EdgeType::Enables
            },
            hardness: "hard".to_string(),
            reason: if candidate.disabled {
                "Config explicitly disables plugin.".to_string()
            } else {
                "Config leaves plugin enabled.".to_string()
            },
        });
        for compatibility in &plugin_system.compatibility {
            edges.push(GraphEdge {
                from: plugin_id.clone(),
                to: format!("tool:{compatibility}"),
                edge_type: EdgeType::CompatibleWith,
                hardness: "soft".to_string(),
                reason: "Catalog declares compatibility.".to_string(),
            });
        }

        if let Some(manifest) = &candidate.manifest_path {
            let artifact_id = stable_id("plugin_artifact", &manifest.to_string_lossy());
            nodes.push(GraphNode::PluginArtifact(PluginArtifactNode {
                id: artifact_id.clone(),
                plugin_id: plugin_id.clone(),
                path: manifest.to_string_lossy().to_string(),
                display_path: display_path(manifest, &config.home_dir),
                artifact_type: ArtifactType::PluginManifest,
                states: vec![NodeState::Declared, NodeState::Effective],
                confidence: 0.95,
                reason: format!(
                    "Plugin manifest detected via {}.",
                    candidate.discovery_sources.join(", ")
                ),
            }));
            edges.push(GraphEdge {
                from: plugin_id.clone(),
                to: artifact_id.clone(),
                edge_type: EdgeType::ProvidesArtifact,
                hardness: "hard".to_string(),
                reason: "Plugin manifest belongs to plugin.".to_string(),
            });
        }
        if let Some(readme) = &candidate.readme_path {
            let artifact_id = stable_id("plugin_artifact", &readme.to_string_lossy());
            nodes.push(GraphNode::PluginArtifact(PluginArtifactNode {
                id: artifact_id.clone(),
                plugin_id: plugin_id.clone(),
                path: readme.to_string_lossy().to_string(),
                display_path: display_path(readme, &config.home_dir),
                artifact_type: ArtifactType::PluginDoc,
                states: vec![NodeState::Declared],
                confidence: 0.7,
                reason: "Plugin documentation detected.".to_string(),
            }));
            edges.push(GraphEdge {
                from: plugin_id.clone(),
                to: artifact_id.clone(),
                edge_type: EdgeType::ProvidesArtifact,
                hardness: "soft".to_string(),
                reason: "Plugin doc belongs to plugin.".to_string(),
            });
        }
        nodes.push(plugin_node);
    }
    Ok((nodes, edges, verdicts))
}

fn discover_plugins(
    config: &AppConfig,
    plugin_system: &crate::domain::PluginSystemCatalog,
    repo_root: &Path,
    config_paths: &[PathBuf],
) -> Result<Vec<PluginCandidate>> {
    let mut candidates = HashMap::<String, PluginCandidate>::new();
    match plugin_system.system.as_str() {
        "codex" => {
            let mut roots = plugin_system
                .install_roots
                .iter()
                .map(|path| resolve_catalog_path(path, &config.home_dir, Some(repo_root)))
                .collect::<Vec<_>>();
            roots.push(
                config
                    .home_dir
                    .join(".codex")
                    .join(".tmp")
                    .join("plugins")
                    .join("plugins"),
            );
            for plugin_root in discover_plugin_roots(&roots, ".codex-plugin", config.scan_max_depth + 3)
            {
                merge_plugin_candidate(
                    &mut candidates,
                    plugin_candidate(&plugin_root, ".codex-plugin", "cache_layout", config_paths),
                );
            }
        }
        "claude" => {
            let installed_path = config
                .home_dir
                .join(".claude")
                .join("plugins")
                .join("installed_plugins.json");
            for plugin_root in read_claude_installed_paths(&installed_path) {
                merge_plugin_candidate(
                    &mut candidates,
                    plugin_candidate(&plugin_root, ".claude-plugin", "install_index", config_paths),
                );
            }
            let mut roots = plugin_system
                .install_roots
                .iter()
                .map(|path| resolve_catalog_path(path, &config.home_dir, Some(repo_root)))
                .collect::<Vec<_>>();
            roots.push(config.home_dir.join(".claude").join("plugins").join("marketplaces"));
            roots.push(config.home_dir.join(".claude").join("plugins").join("cache"));
            for plugin_root in discover_plugin_roots(&roots, ".claude-plugin", config.scan_max_depth + 4)
            {
                let source = if plugin_root.to_string_lossy().contains("/marketplaces/") {
                    "marketplace_layout"
                } else {
                    "cache_layout"
                };
                merge_plugin_candidate(
                    &mut candidates,
                    plugin_candidate(&plugin_root, ".claude-plugin", source, config_paths),
                );
            }
        }
        _ => {}
    }
    let mut values = candidates.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(values)
}

fn discover_plugin_roots(roots: &[PathBuf], marker_dir: &str, max_depth: usize) -> Vec<PathBuf> {
    let mut discovered = HashSet::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_dir())
        {
            if entry.file_name().to_string_lossy() == marker_dir {
                if let Some(parent) = entry.path().parent() {
                    discovered.insert(parent.to_path_buf());
                }
            }
        }
    }
    let mut roots = discovered.into_iter().collect::<Vec<_>>();
    roots.sort();
    roots
}

fn plugin_candidate(
    plugin_root: &Path,
    marker_dir: &str,
    discovery_source: &str,
    config_paths: &[PathBuf],
) -> PluginCandidate {
    let manifest_path = [
        plugin_root.join(marker_dir).join("plugin.json"),
        plugin_root.join("plugin.json"),
        plugin_root.join("package.json"),
    ]
    .into_iter()
    .find(|path| path.exists());
    let plugin_name = manifest_path
        .as_ref()
        .and_then(|path| read_plugin_name(path))
        .or_else(|| {
            plugin_root
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "unknown-plugin".to_string());
    PluginCandidate {
        key: plugin_name.clone(),
        name: plugin_name.clone(),
        install_root: plugin_root.to_path_buf(),
        manifest_path,
        readme_path: plugin_root.join("README.md").exists().then(|| plugin_root.join("README.md")),
        discovery_sources: vec![discovery_source.to_string()],
        disabled: plugin_disabled(&plugin_name, config_paths),
    }
}

fn merge_plugin_candidate(
    candidates: &mut HashMap<String, PluginCandidate>,
    candidate: PluginCandidate,
) {
    let key = candidate.key.clone();
    if let Some(existing) = candidates.get_mut(&key) {
        if existing.manifest_path.is_none() {
            existing.manifest_path = candidate.manifest_path.clone();
        }
        if existing.readme_path.is_none() {
            existing.readme_path = candidate.readme_path.clone();
        }
        if candidate.install_root.to_string_lossy().len() < existing.install_root.to_string_lossy().len() {
            existing.install_root = candidate.install_root.clone();
        }
        existing.disabled = existing.disabled || candidate.disabled;
        for source in candidate.discovery_sources {
            if !existing.discovery_sources.contains(&source) {
                existing.discovery_sources.push(source);
            }
        }
        return;
    }
    candidates.insert(key, candidate);
}

fn read_claude_installed_paths(path: &Path) -> Vec<PathBuf> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    value
        .get("plugins")
        .and_then(|plugins| plugins.as_object())
        .map(|plugins| {
            plugins
                .values()
                .flat_map(|entries| entries.as_array().into_iter().flatten())
                .filter_map(|entry| entry.get("installPath").and_then(|value| value.as_str()))
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn collect_reference_edges(
    nodes: &mut BTreeMap<String, GraphNode>,
    tool_node: &GraphNode,
    config: &AppConfig,
    indexed_at: DateTime<Utc>,
) -> Result<ReferenceCollection> {
    let mut collection = ReferenceCollection::default();
    let mut existing_path_to_id = nodes
        .values()
        .filter_map(path_node_id)
        .collect::<HashMap<_, _>>();
    let mut artifact_nodes = nodes
        .values()
        .filter_map(artifact_node_from_graph)
        .collect::<VecDeque<_>>();
    let mut scanned_ids = HashSet::new();

    while let Some(artifact) = artifact_nodes.pop_front() {
        if !scanned_ids.insert(artifact.id.clone()) {
            continue;
        }
        let path = Path::new(&artifact.path);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let context = ResolverContext {
            base_file: path,
            base_display_path: &artifact.display_path,
            artifact_type: &artifact.artifact_type,
            tool_family: &artifact.tool_family,
            home_dir: &config.home_dir,
        };
        for hit in extract_references(&context, &content) {
            let target_path = hit.resolved_path.to_string_lossy().to_string();
            let target_id = existing_path_to_id
                .get(&target_path)
                .cloned()
                .unwrap_or_else(|| stable_id("reference", &target_path));
            let target_display_path = display_path(&hit.resolved_path, &config.home_dir);
            if !nodes.contains_key(&target_id) {
                let target_reason = if hit.broken {
                    format!("{} Missing target: {}", hit.reason, target_display_path)
                } else {
                    format!("{} Target: {}", hit.reason, target_display_path)
                };
                nodes.insert(
                    target_id.clone(),
                    GraphNode::Artifact(ArtifactNode {
                        id: target_id.clone(),
                        path: hit.resolved_path.to_string_lossy().to_string(),
                        display_path: target_display_path.clone(),
                        artifact_type: ArtifactType::ReferenceTarget,
                        tool_family: "reference".to_string(),
                        scope_type: if hit.broken {
                            ScopeType::Imported
                        } else {
                            ScopeType::Repo
                        },
                        states: if hit.broken {
                            vec![NodeState::BrokenReference, NodeState::Unresolved]
                        } else {
                            vec![NodeState::ReferencedOnly]
                        },
                        confidence: hit.confidence,
                        origin: "reference".to_string(),
                        last_indexed_at: indexed_at,
                        hash: file_hash(&hit.resolved_path)
                            .unwrap_or_else(|_| "missing".to_string()),
                        mtime: fs::metadata(&hit.resolved_path).ok().and_then(mtime_utc),
                        reason: target_reason.clone(),
                    }),
                );
                existing_path_to_id.insert(target_path.clone(), target_id.clone());
                collection.verdicts.push(node_verdict(
                    &target_id,
                    if hit.broken {
                        &[NodeState::BrokenReference, NodeState::Unresolved]
                    } else {
                        &[NodeState::ReferencedOnly]
                    },
                    &target_reason,
                ));
            }
            if let Some(target_artifact) = nodes.get(&target_id).and_then(artifact_node_from_graph) {
                artifact_nodes.push_back(target_artifact);
            }
            collection.edges.push(GraphEdge {
                from: artifact.id.clone(),
                to: target_id.clone(),
                edge_type: hit.edge_type,
                hardness: if hit.broken { "soft" } else { "hard" }.to_string(),
                reason: if hit.broken {
                    format!(
                        "{} Broken target from {}.",
                        hit.reason, artifact.display_path
                    )
                } else {
                    format!("{} Source: {}.", hit.reason, artifact.display_path)
                },
            });
            collection.edges.push(GraphEdge {
                from: tool_node.id().to_string(),
                to: target_id.clone(),
                edge_type: EdgeType::References,
                hardness: "soft".to_string(),
                reason: "Tool context reaches reference target through artifact graph.".to_string(),
            });
            if hit.promotes_effective && !hit.broken {
                collection.promotable_edges.push(PromotableEdge {
                    from: artifact.id.clone(),
                    to: target_id,
                    reason: hit.reason,
                });
            }
        }
    }
    Ok(collection)
}

fn artifact_node_from_graph(node: &GraphNode) -> Option<ArtifactNode> {
    match node {
        GraphNode::Artifact(artifact) => Some(artifact.clone()),
        GraphNode::PluginArtifact(artifact) => Some(ArtifactNode {
            id: artifact.id.clone(),
            path: artifact.path.clone(),
            display_path: artifact.display_path.clone(),
            artifact_type: artifact.artifact_type.clone(),
            tool_family: "plugin".to_string(),
            scope_type: ScopeType::PluginProvided,
            states: artifact.states.clone(),
            confidence: artifact.confidence,
            origin: "plugin".to_string(),
            last_indexed_at: Utc::now(),
            hash: file_hash(Path::new(&artifact.path)).unwrap_or_else(|_| "missing".to_string()),
            mtime: None,
            reason: artifact.reason.clone(),
        }),
        _ => None,
    }
}

fn path_node_id(node: &GraphNode) -> Option<(String, String)> {
    match node {
        GraphNode::Artifact(artifact) => Some((artifact.path.clone(), artifact.id.clone())),
        GraphNode::PluginArtifact(artifact) => Some((artifact.path.clone(), artifact.id.clone())),
        _ => None,
    }
}

fn promote_effective_closure(
    nodes: &mut BTreeMap<String, GraphNode>,
    edges: &[GraphEdge],
    verdicts: &mut Vec<Verdict>,
    promotable_reference_edges: &[PromotableEdge],
) {
    let promotable_refs = promotable_reference_edges
        .iter()
        .map(|edge| ((edge.from.clone(), edge.to.clone()), edge.reason.clone()))
        .collect::<HashMap<_, _>>();
    let mut adjacency = HashMap::<String, Vec<(String, String)>>::new();
    for edge in edges {
        let reason = if matches!(edge.edge_type, EdgeType::Imports | EdgeType::Loads) {
            Some(edge.reason.clone())
        } else if matches!(edge.edge_type, EdgeType::References) {
            promotable_refs
                .get(&(edge.from.clone(), edge.to.clone()))
                .cloned()
        } else {
            None
        };
        if let Some(reason) = reason {
            adjacency
                .entry(edge.from.clone())
                .or_default()
                .push((edge.to.clone(), reason));
        }
    }

    let mut queue = VecDeque::new();
    let mut seed_paths = HashMap::<String, Vec<String>>::new();
    for (node_id, node) in nodes.iter() {
        if node.states().contains(&NodeState::Effective) {
            queue.push_back(node_id.clone());
            seed_paths.insert(node_id.clone(), vec![node_id.clone()]);
        }
    }

    let mut expanded = HashSet::new();
    while let Some(source_id) = queue.pop_front() {
        if !expanded.insert(source_id.clone()) {
            continue;
        }
        let Some(source_path) = seed_paths.get(&source_id).cloned() else {
            continue;
        };
        let source_label = nodes
            .get(&source_id)
            .map(GraphNode::label)
            .unwrap_or_else(|| source_id.clone());
        for (target_id, reason) in adjacency.get(&source_id).cloned().unwrap_or_default() {
            if source_id == target_id {
                continue;
            }
            let Some(target_node) = nodes.get_mut(&target_id) else {
                continue;
            };
            if node_is_broken(target_node) {
                continue;
            }
            let mut promoted = false;
            let target_label = target_node.label();
            match target_node {
                GraphNode::Artifact(artifact) => {
                    promoted = promote_node_states(&mut artifact.states, &mut artifact.confidence);
                    if promoted {
                        artifact.reason = format!(
                            "{} Included transitively from {}.",
                            artifact.reason, source_label
                        );
                    }
                }
                GraphNode::PluginArtifact(artifact) => {
                    promoted = promote_node_states(&mut artifact.states, &mut artifact.confidence);
                    if promoted {
                        artifact.reason = format!(
                            "{} Included transitively from {}.",
                            artifact.reason, source_label
                        );
                    }
                }
                _ => {}
            }

            let mut path = source_path.clone();
            path.push(target_id.clone());
            let verdict = upsert_verdict(verdicts, &target_id, target_node.states());
            if promoted {
                verdict.why_included.push(format!(
                    "Effective via {} -> {} ({reason})",
                    source_label, target_label
                ));
                verdict.provenance_paths.push(path.clone());
                queue.push_back(target_id.clone());
            }
            if !seed_paths.contains_key(&target_id)
                || seed_paths[&target_id].len() > path.len()
            {
                seed_paths.insert(target_id, path);
            }
        }
    }
}

fn node_is_broken(node: &GraphNode) -> bool {
    node.states().contains(&NodeState::BrokenReference)
        || node.states().contains(&NodeState::Unresolved)
}

fn promote_node_states(states: &mut Vec<NodeState>, confidence: &mut f32) -> bool {
    if states.contains(&NodeState::Effective) {
        return false;
    }
    states.retain(|state| *state != NodeState::ReferencedOnly);
    states.push(NodeState::Effective);
    *confidence = confidence.max(0.92);
    true
}

fn upsert_verdict<'a>(
    verdicts: &'a mut Vec<Verdict>,
    entity_id: &str,
    states: Vec<NodeState>,
) -> &'a mut Verdict {
    if let Some(index) = verdicts.iter().position(|verdict| verdict.entity_id == entity_id) {
        let verdict = &mut verdicts[index];
        verdict.states = states;
        return verdict;
    }
    verdicts.push(Verdict {
        entity_id: entity_id.to_string(),
        states,
        why_included: Vec::new(),
        why_excluded: Vec::new(),
        shadowed_by: Vec::new(),
        provenance_paths: Vec::new(),
    });
    verdicts.last_mut().expect("verdict inserted")
}

#[allow(dead_code)]
fn collect_repo_files(root: &Path, max_depth: usize) -> Vec<String> {
    collect_repo_files_with_progress(root, max_depth, Path::new(""), &mut |_| Ok(()))
        .unwrap_or_default()
}

fn collect_repo_files_with_progress(
    root: &Path,
    max_depth: usize,
    home_dir: &Path,
    on_progress: &mut dyn FnMut(String) -> Result<()>,
) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != root)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy();
            !matches!(name.as_ref(), ".git" | "node_modules" | "target" | "dist")
        })
    {
        if entry.file_type().is_dir() {
            let display = if home_dir == Path::new("") {
                entry.path().to_string_lossy().to_string()
            } else {
                display_path(entry.path(), home_dir)
            };
            on_progress(display)?;
            continue;
        }
        if entry.file_type().is_file() {
            if let Ok(path) = entry.path().strip_prefix(root) {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }
    Ok(files)
}

#[allow(dead_code)]
fn discover_repos(roots: &[PathBuf], max_depth: usize) -> Vec<PathBuf> {
    discover_repos_with_progress(roots, max_depth, Path::new(""), &mut |_| Ok(()))
        .unwrap_or_default()
}

fn discover_repos_with_progress(
    roots: &[PathBuf],
    max_depth: usize,
    home_dir: &Path,
    on_progress: &mut dyn FnMut(ScanProgress) -> Result<()>,
) -> Result<Vec<PathBuf>> {
    let mut repos = HashSet::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
        let root_display_path = if home_dir == Path::new("") {
            root.to_string_lossy().to_string()
        } else {
            display_path(root, home_dir)
        };
        on_progress(ScanProgress {
            phase: "root".to_string(),
            message: format!("Scanning root {root_display_path}"),
            current_path: Some(root_display_path.clone()),
            items_done: None,
            items_total: None,
        })?;
        if root.join(".git").exists() {
            repos.insert(root.clone());
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_dir())
        {
            let current_path = if home_dir == Path::new("") {
                entry.path().to_string_lossy().to_string()
            } else {
                display_path(entry.path(), home_dir)
            };
            on_progress(ScanProgress {
                phase: "root".to_string(),
                message: format!("Scanning {current_path}"),
                current_path: Some(current_path),
                items_done: None,
                items_total: None,
            })?;
            if entry.path().join(".git").exists() {
                repos.insert(entry.path().to_path_buf());
            }
        }
    }
    let mut repos = repos.into_iter().collect::<Vec<_>>();
    repos.sort();
    Ok(repos)
}

fn stable_id(prefix: &str, input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{prefix}:{:x}", hasher.finalize())
}

fn file_hash(path: &Path) -> Result<String> {
    if path.is_dir() {
        return Ok("directory".to_string());
    }
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn mtime_utc(metadata: fs::Metadata) -> Option<DateTime<Utc>> {
    metadata.modified().ok().map(DateTime::<Utc>::from)
}

fn resolve_catalog_path(pattern: &str, home_dir: &Path, project_root: Option<&Path>) -> PathBuf {
    let with_home = pattern.replace('~', &home_dir.to_string_lossy());
    let with_project = if let Some(project_root) = project_root {
        with_home.replace("{project}", &project_root.to_string_lossy())
    } else {
        with_home
    };
    PathBuf::from(with_project)
}

fn display_path(path: &Path, home_dir: &Path) -> String {
    if let Ok(relative) = path.strip_prefix(home_dir) {
        let relative = relative.to_string_lossy();
        if relative.is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", relative)
        }
    } else {
        path.to_string_lossy().to_string()
    }
}

#[cfg(test)]
pub(crate) fn build_surface_state_for_test(
    config: &AppConfig,
    store: &Store,
    project: &ProjectSummary,
    catalog: &ToolCatalog,
    inventory: &[String],
) -> Result<SurfaceState> {
    build_surface_state(config, store, project, catalog, inventory)
}

#[cfg(test)]
pub(crate) fn collect_repo_files_for_test(root: &Path, max_depth: usize) -> Vec<String> {
    collect_repo_files(root, max_depth)
}

#[cfg(test)]
pub(crate) fn display_path_for_test(path: &Path, home_dir: &Path) -> String {
    display_path(path, home_dir)
}

fn confidence_from_states(states: &[NodeState]) -> f32 {
    if states.contains(&NodeState::Observed) {
        0.98
    } else if states.contains(&NodeState::Effective) {
        0.92
    } else if states.contains(&NodeState::BrokenReference) {
        0.84
    } else {
        0.7
    }
}

fn node_verdict(entity_id: &str, states: &[NodeState], reason: &str) -> Verdict {
    Verdict {
        entity_id: entity_id.to_string(),
        states: states.to_vec(),
        why_included: vec![reason.to_string()],
        why_excluded: Vec::new(),
        shadowed_by: Vec::new(),
        provenance_paths: vec![vec![entity_id.to_string()]],
    }
}

fn dedupe_edges(edges: Vec<GraphEdge>) -> Vec<GraphEdge> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();
    for edge in edges {
        let key = format!("{}:{}:{:?}", edge.from, edge.to, edge.edge_type);
        if seen.insert(key) {
            deduped.push(edge);
        }
    }
    deduped
}

fn read_plugin_name(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
        return value
            .get("name")
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
    }
    None
}

fn plugin_disabled(plugin_name: &str, config_paths: &[PathBuf]) -> bool {
    config_paths.iter().any(|path| {
        if !path.exists() {
            return false;
        }
        let Ok(text) = fs::read_to_string(path) else {
            return false;
        };
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("toml") => toml::from_str::<toml::Value>(&text)
                .ok()
                .and_then(|value| plugin_enabled_flag_toml(&value, plugin_name))
                .is_some_and(|enabled| !enabled),
            Some("json") => serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|value| plugin_enabled_flag_json(&value, plugin_name))
                .is_some_and(|enabled| !enabled),
            _ => false,
        }
    })
}

fn plugin_enabled_flag_toml(value: &toml::Value, plugin_name: &str) -> Option<bool> {
    value
        .get("plugins")
        .and_then(|plugins| plugins.as_table())
        .and_then(|plugins| {
            plugins.iter().find_map(|(key, entry)| {
                plugin_alias_matches(key, plugin_name)
                    .then(|| entry.get("enabled").and_then(|flag| flag.as_bool()))
                    .flatten()
            })
        })
}

fn plugin_enabled_flag_json(value: &serde_json::Value, plugin_name: &str) -> Option<bool> {
    value
        .get("plugins")
        .and_then(|plugins| plugins.as_object())
        .and_then(|plugins| {
            plugins.iter().find_map(|(key, entry)| {
                plugin_alias_matches(key, plugin_name)
                    .then(|| entry.get("enabled").and_then(|flag| flag.as_bool()))
                    .flatten()
            })
        })
}

fn plugin_alias_matches(config_key: &str, plugin_name: &str) -> bool {
    config_key == plugin_name
        || config_key
            .split('@')
            .next()
            .is_some_and(|prefix| prefix == plugin_name)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::Utc;
    use tempfile::TempDir;

    use crate::{
        catalogs::seed_catalog_map,
        config::AppConfig,
        domain::{GraphNode, NodeState},
        storage::Store,
    };

    use super::{
        build_surface_state, collect_repo_files, display_path, scan_projects,
        scan_projects_with_progress,
    };

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
        assert!(progress.iter().any(|update| update.phase == "root"));
        assert!(progress.iter().any(|update| update.phase == "repo"));
        assert!(progress.iter().any(|update| update.phase == "walk"));
        assert!(progress
            .iter()
            .any(|update| update.current_path.as_deref() == Some("~/git/demo/docs")));
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
            &crate::domain::ProjectSummary {
                id: "demo".to_string(),
                root_path: repo.to_string_lossy().to_string(),
                display_path: display_path(&repo, &home),
                name: "demo".to_string(),
                indexed_at: Utc::now(),
                status: "ready".to_string(),
            },
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
            &crate::domain::ProjectSummary {
                id: "demo".to_string(),
                root_path: repo.to_string_lossy().to_string(),
                display_path: display_path(&repo, &home),
                name: "demo".to_string(),
                indexed_at: Utc::now(),
                status: "ready".to_string(),
            },
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
            &crate::domain::ProjectSummary {
                id: "demo".to_string(),
                root_path: repo.to_string_lossy().to_string(),
                display_path: display_path(&repo, &home),
                name: "demo".to_string(),
                indexed_at: Utc::now(),
                status: "ready".to_string(),
            },
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
            &crate::domain::ProjectSummary {
                id: "demo".to_string(),
                root_path: repo.to_string_lossy().to_string(),
                display_path: display_path(&repo, &home),
                name: "demo".to_string(),
                indexed_at: Utc::now(),
                status: "ready".to_string(),
            },
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
            &crate::domain::ProjectSummary {
                id: "demo".to_string(),
                root_path: repo.to_string_lossy().to_string(),
                display_path: display_path(&repo, &home),
                name: "demo".to_string(),
                indexed_at: Utc::now(),
                status: "ready".to_string(),
            },
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
            &crate::domain::ProjectSummary {
                id: "demo".to_string(),
                root_path: repo.to_string_lossy().to_string(),
                display_path: display_path(&repo, &home),
                name: "demo".to_string(),
                indexed_at: Utc::now(),
                status: "ready".to_string(),
            },
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
            &crate::domain::ProjectSummary {
                id: "demo".to_string(),
                root_path: repo.to_string_lossy().to_string(),
                display_path: display_path(&repo, &home),
                name: "demo".to_string(),
                indexed_at: Utc::now(),
                status: "ready".to_string(),
            },
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
        assert!(plugin.discovery_sources.iter().any(|source| source == "cache_layout"));
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
            &crate::domain::ProjectSummary {
                id: "demo".to_string(),
                root_path: repo.to_string_lossy().to_string(),
                display_path: display_path(&repo, &home),
                name: "demo".to_string(),
                indexed_at: Utc::now(),
                status: "ready".to_string(),
            },
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
        assert!(plugin
            .discovery_sources
            .iter()
            .any(|source| source == "install_index"));
    }
}
