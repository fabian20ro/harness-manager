use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSetBuilder};
use serde_json::{Map as JsonMap, Value as JsonValue};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{
    catalogs::{catalog_path, seed_catalogs},
    config::AppConfig,
    domain::{
        ArtifactNode, ArtifactRule, ArtifactType, EdgeType, GraphEdge, GraphNode, NodeState,
        PluginArtifactNode, PluginNode, ProjectDiscoveryRootStrategy, ProjectDiscoveryRule,
        ProjectKind, ProjectNode, ProjectSummary, RemoteSnapshotNode, ScopeType,
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
    manifest_base_dir: Option<PathBuf>,
    readme_path: Option<PathBuf>,
    discovery_sources: Vec<String>,
    disabled: bool,
}

#[derive(Clone, Debug)]
struct ProjectCandidate {
    root_path: PathBuf,
    name: String,
    kind: ProjectKind,
    discovery_reason: String,
    signal_score: i32,
}

#[derive(Clone, Debug)]
struct CandidateSignal {
    root_path: PathBuf,
    kind: ProjectKind,
    score: i32,
    reason: String,
}

#[derive(Clone, Debug)]
struct CompiledProjectDiscoveryRule {
    kind: ProjectKind,
    score: i32,
    reason: String,
    root_strategy: ProjectDiscoveryRootStrategy,
    skip_if_scan_root: bool,
    matcher: globset::GlobMatcher,
}

#[derive(Clone, Debug, Default)]
struct ParsedSkillMetadata {
    name: Option<String>,
    description: Option<String>,
    metadata: Option<JsonValue>,
}

#[derive(Clone, Debug)]
struct ScannableArtifact {
    id: String,
    path: String,
    display_path: String,
    resolve_from_dir: PathBuf,
    artifact_type: ArtifactType,
    tool_family: String,
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
    let project_candidates = discover_project_candidates_with_progress(
        &roots,
        &config.known_global_dirs,
        config.scan_max_depth,
        &catalogs,
        &config.home_dir,
        &mut on_progress,
    )?;

    let mut summaries = Vec::new();
    let total_projects = project_candidates.len();
    for (index, candidate) in project_candidates.iter().enumerate() {
        let project_display_path = display_path(&candidate.root_path, &config.home_dir);
        on_progress(ScanProgress {
            phase: "repo".to_string(),
            message: format!("Indexing {} {project_display_path}", project_kind_label(&candidate.kind)),
            current_path: Some(project_display_path.clone()),
            items_done: Some(index),
            items_total: Some(total_projects),
        })?;
        let summary = scan_project(
            config,
            store,
            &catalogs,
            candidate,
            index + 1,
            total_projects,
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

pub fn reindex_project_tool_with_progress<F>(
    config: &AppConfig,
    store: &Store,
    project_id: &str,
    tool: &str,
    mut on_progress: F,
) -> Result<SurfaceState>
where
    F: FnMut(ScanProgress) -> Result<()>,
{
    store.ensure_layout()?;
    let mut projects = store
        .maybe_read_json::<Vec<ProjectSummary>>(&store.projects_index_path())?
        .unwrap_or_default();
    let project_index = projects
        .iter()
        .position(|project| project.id == project_id)
        .context("project not found in index")?;
    let project = projects[project_index].clone();
    let repo_root = PathBuf::from(&project.root_path);
    let repo_display_path = display_path(&repo_root, &config.home_dir);
    let catalogs = load_catalogs(store)?;
    let catalog = catalogs.get(tool).context("tool catalog not found")?;

    on_progress(ScanProgress {
        phase: "repo".to_string(),
        message: format!("Reindexing {} for {}", catalog.display_name, repo_display_path),
        current_path: Some(repo_display_path.clone()),
        items_done: Some(0),
        items_total: Some(1),
    })?;

    let inventory = collect_repo_files_with_progress(
        &repo_root,
        config.scan_max_depth,
        &config.home_dir,
        &mut |current_dir| {
            on_progress(ScanProgress {
                phase: "walk".to_string(),
                message: format!("Scanning {current_dir}"),
                current_path: Some(current_dir),
                items_done: Some(0),
                items_total: Some(1),
            })
        },
    )?;
    store.write_json(&store.inventory_path(project_id), &inventory)?;

    let updated_project = ProjectSummary {
        indexed_at: Utc::now(),
        ..project
    };
    on_progress(ScanProgress {
        phase: "surface".to_string(),
        message: format!(
            "Evaluating {} for {}",
            catalog.display_name, repo_display_path
        ),
        current_path: Some(repo_display_path),
        items_done: Some(1),
        items_total: Some(1),
    })?;
    let state = build_surface_state(config, store, &updated_project, catalog, &inventory)?;
    store.write_json(&store.tool_state_path(project_id, tool), &state)?;

    projects[project_index] = updated_project;
    store.write_json(&store.projects_index_path(), &projects)?;
    rewrite_project_union_graph(store, project_id)?;

    Ok(state)
}

fn scan_project(
    config: &AppConfig,
    store: &Store,
    catalogs: &HashMap<String, ToolCatalog>,
    candidate: &ProjectCandidate,
    repo_index: usize,
    total_repos: usize,
    on_progress: &mut dyn FnMut(ScanProgress) -> Result<()>,
) -> Result<ProjectSummary> {
    let indexed_at = Utc::now();
    let project_id = stable_id("project", &candidate.root_path.to_string_lossy());
    let summary = ProjectSummary {
        id: project_id.clone(),
        root_path: candidate.root_path.to_string_lossy().to_string(),
        display_path: display_path(&candidate.root_path, &config.home_dir),
        name: candidate.name.clone(),
        kind: candidate.kind.clone(),
        discovery_reason: candidate.discovery_reason.clone(),
        signal_score: candidate.signal_score,
        indexed_at,
        status: "ready".to_string(),
    };

    let project_dir = store.project_dir(&project_id);
    fs::create_dir_all(project_dir.join("tool-state"))?;
    let repo_display_path = display_path(&candidate.root_path, &config.home_dir);
    let inventory = collect_repo_files_with_progress(
        &candidate.root_path,
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

fn rewrite_project_union_graph(store: &Store, project_id: &str) -> Result<()> {
    let mut union_nodes = BTreeMap::<String, GraphNode>::new();
    let mut union_edges = Vec::<GraphEdge>::new();
    let tool_state_dir = store.project_dir(project_id).join("tool-state");

    if tool_state_dir.exists() {
        let mut paths = fs::read_dir(&tool_state_dir)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "json"))
            .collect::<Vec<_>>();
        paths.sort();

        for path in paths {
            let state = store.read_json::<SurfaceState>(&path)?;
            for node in state.nodes {
                union_nodes.insert(node.id().to_string(), node);
            }
            union_edges.extend(state.edges);
        }
    }

    store.write_json(
        &store.graph_nodes_path(project_id),
        &union_nodes.into_values().collect::<Vec<_>>(),
    )?;
    store.write_json(&store.graph_edges_path(project_id), &dedupe_edges(union_edges))?;
    Ok(())
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
                name: None,
                description: None,
                metadata: None,
                resolve_from_path: Some(candidate.install_root.to_string_lossy().to_string()),
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

            if plugin_system.system == "codex" {
                for skill_artifact in discover_codex_skill_artifacts(
                    manifest,
                    &candidate.install_root,
                    &plugin_id,
                    candidate.disabled,
                    &config.home_dir,
                )? {
                    let skill_id = skill_artifact.id.clone();
                    let skill_states = skill_artifact.states.clone();
                    let skill_reason = skill_artifact.reason.clone();
                    nodes.push(GraphNode::PluginArtifact(skill_artifact));
                    verdicts.push(node_verdict(&skill_id, &skill_states, &skill_reason));
                    edges.push(GraphEdge {
                        from: plugin_id.clone(),
                        to: skill_id,
                        edge_type: EdgeType::ProvidesArtifact,
                        hardness: "hard".to_string(),
                        reason: "Plugin bundles skill artifact.".to_string(),
                    });
                }
            }
        }
        if let Some(readme) = &candidate.readme_path {
            let artifact_id = stable_id("plugin_artifact", &readme.to_string_lossy());
            nodes.push(GraphNode::PluginArtifact(PluginArtifactNode {
                id: artifact_id.clone(),
                plugin_id: plugin_id.clone(),
                path: readme.to_string_lossy().to_string(),
                display_path: display_path(readme, &config.home_dir),
                name: None,
                description: None,
                metadata: None,
                resolve_from_path: Some(candidate.install_root.to_string_lossy().to_string()),
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
    let mut roots = plugin_system
        .install_roots
        .iter()
        .map(|path| resolve_catalog_path(path, &config.home_dir, Some(repo_root)))
        .map(|path| (path, "install_root"))
        .collect::<Vec<_>>();

    for candidate in discover_plugin_candidates(
        &roots,
        &plugin_system.manifest_paths,
        config.scan_max_depth + 4,
        config_paths,
    ) {
        merge_plugin_candidate(&mut candidates, candidate);
    }

    match plugin_system.system.as_str() {
        "codex" => {
            roots.push((
                config
                    .home_dir
                    .join(".codex")
                    .join(".tmp")
                    .join("plugins")
                    .join("plugins"),
                "cache_layout",
            ));
            for candidate in discover_plugin_candidates(
                &roots,
                &plugin_system.manifest_paths,
                config.scan_max_depth + 4,
                config_paths,
            ) {
                merge_plugin_candidate(&mut candidates, candidate);
            }
        }
        "claude" => {
            let installed_path = config
                .home_dir
                .join(".claude")
                .join("plugins")
                .join("installed_plugins.json");
            let installed_roots = read_claude_installed_paths(&installed_path)
                .into_iter()
                .map(|path| (path, "install_index"))
                .collect::<Vec<_>>();
            for candidate in discover_plugin_candidates(
                &installed_roots,
                &plugin_system.manifest_paths,
                config.scan_max_depth + 4,
                config_paths,
            ) {
                merge_plugin_candidate(&mut candidates, candidate);
            }
            roots.push((
                config.home_dir.join(".claude").join("plugins").join("marketplaces"),
                "marketplace_layout",
            ));
            roots.push((
                config.home_dir.join(".claude").join("plugins").join("cache"),
                "cache_layout",
            ));
            for candidate in discover_plugin_candidates(
                &roots,
                &plugin_system.manifest_paths,
                config.scan_max_depth + 5,
                config_paths,
            ) {
                merge_plugin_candidate(&mut candidates, candidate);
            }
        }
        _ => {}
    }
    let mut values = candidates.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(values)
}

fn discover_plugin_candidates(
    roots: &[(PathBuf, &str)],
    manifest_paths: &[String],
    max_depth: usize,
    config_paths: &[PathBuf],
) -> Vec<PluginCandidate> {
    let manifest_paths = manifest_paths
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.components().count() > 0)
        .collect::<Vec<_>>();
    let mut discovered = HashMap::<(PathBuf, PathBuf), PluginCandidate>::new();
    for (root, discovery_source) in roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            for manifest_suffix in &manifest_paths {
                if entry.path().ends_with(manifest_suffix) {
                    if let Some(plugin_root) =
                        plugin_root_from_manifest(entry.path(), manifest_suffix)
                    {
                        let candidate = plugin_candidate(
                            &plugin_root,
                            entry.path(),
                            discovery_source,
                            config_paths,
                        );
                        discovered.insert(
                            (candidate.install_root.clone(), entry.path().to_path_buf()),
                            candidate,
                        );
                    }
                }
            }
        }
    }
    let mut candidates = discovered.into_values().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.install_root
            .cmp(&right.install_root)
            .then(left.manifest_path.cmp(&right.manifest_path))
    });
    candidates
}

fn plugin_root_from_manifest(manifest_path: &Path, manifest_suffix: &Path) -> Option<PathBuf> {
    let component_count = manifest_suffix.components().count();
    let mut current = manifest_path.to_path_buf();
    for _ in 0..component_count {
        current = current.parent()?.to_path_buf();
    }
    Some(current)
}

fn plugin_candidate(
    plugin_root: &Path,
    manifest_path: &Path,
    discovery_source: &str,
    config_paths: &[PathBuf],
) -> PluginCandidate {
    let plugin_name = manifest_path
        .exists()
        .then(|| manifest_path.to_path_buf())
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
        manifest_path: Some(manifest_path.to_path_buf()),
        manifest_base_dir: manifest_path.parent().map(Path::to_path_buf),
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
        if existing.manifest_base_dir.is_none() {
            existing.manifest_base_dir = candidate.manifest_base_dir.clone();
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

fn discover_codex_skill_artifacts(
    manifest_path: &Path,
    plugin_root: &Path,
    plugin_id: &str,
    plugin_disabled: bool,
    home_dir: &Path,
) -> Result<Vec<PluginArtifactNode>> {
    let Some(manifest) = read_plugin_manifest(manifest_path) else {
        return Ok(Vec::new());
    };
    let mut artifacts = Vec::new();
    let mut seen = HashSet::new();

    for raw_skill_path in manifest_skill_entries(&manifest) {
        let resolved = resolve_plugin_component_path(plugin_root, &raw_skill_path);
        if !resolved.exists() {
            artifacts.push(PluginArtifactNode {
                id: stable_id("plugin_artifact", &resolved.to_string_lossy()),
                plugin_id: plugin_id.to_string(),
                path: resolved.to_string_lossy().to_string(),
                display_path: display_path(&resolved, home_dir),
                name: None,
                description: None,
                metadata: None,
                resolve_from_path: Some(plugin_root.to_string_lossy().to_string()),
                artifact_type: ArtifactType::Skill,
                states: missing_skill_states(plugin_disabled),
                confidence: 0.92,
                reason: format!("Plugin declares missing skill path: {}.", raw_skill_path),
            });
            continue;
        }

        for skill_path in resolve_skill_paths(&resolved) {
            if !seen.insert(skill_path.clone()) {
                continue;
            }
            artifacts.push(build_skill_artifact(
                plugin_id,
                plugin_root,
                &skill_path,
                plugin_disabled,
                home_dir,
            ));
        }
    }

    artifacts.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(artifacts)
}

fn read_plugin_manifest(path: &Path) -> Option<JsonValue> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<JsonValue>(&text).ok()
}

fn manifest_skill_entries(value: &JsonValue) -> Vec<String> {
    match value.get("skills") {
        Some(JsonValue::String(path)) => vec![path.clone()],
        Some(JsonValue::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(ToString::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn resolve_plugin_component_path(plugin_root: &Path, raw_path: &str) -> PathBuf {
    let relative = raw_path
        .strip_prefix("./")
        .or_else(|| raw_path.strip_prefix(".\\"))
        .unwrap_or(raw_path);
    plugin_root.join(relative)
}

fn resolve_skill_paths(path: &Path) -> Vec<PathBuf> {
    if path.is_file() {
        return (path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md"))
            .then(|| vec![path.to_path_buf()])
            .unwrap_or_default();
    }
    if !path.is_dir() {
        return Vec::new();
    }

    let direct_skill = path.join("SKILL.md");
    if direct_skill.is_file() {
        return vec![direct_skill];
    }

    let mut skill_paths = WalkDir::new(path)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.file_name().to_string_lossy() == "SKILL.md")
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    skill_paths.sort();
    skill_paths
}

fn build_skill_artifact(
    plugin_id: &str,
    plugin_root: &Path,
    skill_path: &Path,
    plugin_disabled: bool,
    home_dir: &Path,
) -> PluginArtifactNode {
    let parsed = parse_skill_metadata(skill_path);
    PluginArtifactNode {
        id: stable_id("plugin_artifact", &skill_path.to_string_lossy()),
        plugin_id: plugin_id.to_string(),
        path: skill_path.to_string_lossy().to_string(),
        display_path: display_path(skill_path, home_dir),
        name: parsed.name,
        description: parsed.description,
        metadata: parsed.metadata,
        resolve_from_path: Some(plugin_root.to_string_lossy().to_string()),
        artifact_type: ArtifactType::Skill,
        states: existing_skill_states(plugin_disabled),
        confidence: if plugin_disabled { 0.82 } else { 0.96 },
        reason: "Plugin bundles skill artifact.".to_string(),
    }
}

fn existing_skill_states(plugin_disabled: bool) -> Vec<NodeState> {
    let mut states = vec![NodeState::Declared];
    if plugin_disabled {
        states.push(NodeState::Inactive);
    } else {
        states.push(NodeState::Effective);
    }
    states
}

fn missing_skill_states(plugin_disabled: bool) -> Vec<NodeState> {
    let mut states = vec![
        NodeState::Declared,
        NodeState::BrokenReference,
        NodeState::Unresolved,
    ];
    if plugin_disabled {
        states.push(NodeState::Inactive);
    }
    states
}

fn parse_skill_metadata(skill_path: &Path) -> ParsedSkillMetadata {
    let Ok(content) = fs::read_to_string(skill_path) else {
        return ParsedSkillMetadata::default();
    };
    let Some(frontmatter) = extract_frontmatter(&content) else {
        return attach_openai_metadata(skill_path, ParsedSkillMetadata::default());
    };
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&frontmatter) else {
        return attach_openai_metadata(skill_path, ParsedSkillMetadata::default());
    };

    let mut metadata = ParsedSkillMetadata {
        name: yaml_field_as_string(&value, "name"),
        description: yaml_field_as_string(&value, "description"),
        metadata: None,
    };

    let legacy = legacy_frontmatter_metadata(&value);
    metadata = attach_openai_metadata(skill_path, metadata);
    if let Some(legacy_value) = legacy {
        merge_skill_metadata(&mut metadata, "legacy_frontmatter", legacy_value);
    }
    metadata
}

fn extract_frontmatter(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut frontmatter = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            return Some(frontmatter.join("\n"));
        }
        frontmatter.push(line);
    }
    None
}

fn yaml_field_as_string(value: &serde_yaml::Value, key: &str) -> Option<String> {
    value
        .as_mapping()
        .and_then(|mapping| mapping.get(&serde_yaml::Value::String(key.to_string())))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn legacy_frontmatter_metadata(value: &serde_yaml::Value) -> Option<JsonValue> {
    let mapping = value.as_mapping()?;
    let mut legacy = JsonMap::new();
    for key in [
        "retrieval",
        "intents",
        "entities",
        "pathPatterns",
        "bashPatterns",
    ] {
        let Some(entry) = mapping.get(&serde_yaml::Value::String(key.to_string())) else {
            continue;
        };
        let Ok(json_value) = serde_json::to_value(entry) else {
            continue;
        };
        legacy.insert(key.to_string(), json_value);
    }
    (!legacy.is_empty()).then(|| JsonValue::Object(legacy))
}

fn attach_openai_metadata(skill_path: &Path, mut metadata: ParsedSkillMetadata) -> ParsedSkillMetadata {
    let skill_dir = skill_path.parent().unwrap_or(skill_path);
    for candidate in [
        skill_dir.join("agents").join("openai.yaml"),
        skill_dir.join("agents").join("openai.yml"),
    ] {
        let Ok(content) = fs::read_to_string(&candidate) else {
            continue;
        };
        let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&content) else {
            continue;
        };
        if let Ok(json_value) = serde_json::to_value(value) {
            merge_skill_metadata(&mut metadata, "openai", json_value);
            break;
        }
    }
    metadata
}

fn merge_skill_metadata(metadata: &mut ParsedSkillMetadata, key: &str, value: JsonValue) {
    let mut root = metadata
        .metadata
        .take()
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    root.insert(key.to_string(), value);
    metadata.metadata = Some(JsonValue::Object(root));
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
            resolve_from_dir: &artifact.resolve_from_dir,
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
                    to: target_id.clone(),
                    reason: hit.reason,
                });
            }
            if !hit.broken && hit.resolved_path.is_dir() {
                materialize_referenced_directory(
                    &target_id,
                    &hit.resolved_path,
                    &target_display_path,
                    nodes,
                    &mut existing_path_to_id,
                    &mut artifact_nodes,
                    &mut collection,
                    indexed_at,
                    &config.home_dir,
                )?;
            }
        }
    }
    Ok(collection)
}

fn materialize_referenced_directory(
    directory_id: &str,
    directory_path: &Path,
    directory_display_path: &str,
    nodes: &mut BTreeMap<String, GraphNode>,
    existing_path_to_id: &mut HashMap<String, String>,
    artifact_nodes: &mut VecDeque<ScannableArtifact>,
    collection: &mut ReferenceCollection,
    indexed_at: DateTime<Utc>,
    home_dir: &Path,
) -> Result<()> {
    let mut file_paths = WalkDir::new(directory_path)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect::<Vec<_>>();
    file_paths.sort();

    for file_path in file_paths {
        let file_path_string = file_path.to_string_lossy().to_string();
        let file_id = existing_path_to_id
            .get(&file_path_string)
            .cloned()
            .unwrap_or_else(|| stable_id("reference", &file_path_string));
        let file_display_path = display_path(&file_path, home_dir);
        if !nodes.contains_key(&file_id) {
            let reason = format!(
                "Contained in referenced directory: {}.",
                directory_display_path
            );
            let states = vec![NodeState::ReferencedOnly];
            nodes.insert(
                file_id.clone(),
                GraphNode::Artifact(ArtifactNode {
                    id: file_id.clone(),
                    path: file_path_string.clone(),
                    display_path: file_display_path.clone(),
                    artifact_type: ArtifactType::ReferenceTarget,
                    tool_family: "reference".to_string(),
                    scope_type: ScopeType::Repo,
                    states: states.clone(),
                    confidence: confidence_from_states(&states),
                    origin: "reference".to_string(),
                    last_indexed_at: indexed_at,
                    hash: file_hash(&file_path).unwrap_or_else(|_| "missing".to_string()),
                    mtime: fs::metadata(&file_path).ok().and_then(mtime_utc),
                    reason: reason.clone(),
                }),
            );
            existing_path_to_id.insert(file_path_string.clone(), file_id.clone());
            collection
                .verdicts
                .push(node_verdict(&file_id, &states, &reason));
        }
        if let Some(target_artifact) = nodes.get(&file_id).and_then(artifact_node_from_graph) {
            artifact_nodes.push_back(target_artifact);
        }
        let reason = format!(
            "Contained in referenced directory: {}.",
            directory_display_path
        );
        collection.edges.push(GraphEdge {
            from: directory_id.to_string(),
            to: file_id.clone(),
            edge_type: EdgeType::References,
            hardness: "hard".to_string(),
            reason: reason.clone(),
        });
        collection.promotable_edges.push(PromotableEdge {
            from: directory_id.to_string(),
            to: file_id,
            reason,
        });
    }

    Ok(())
}

fn artifact_node_from_graph(node: &GraphNode) -> Option<ScannableArtifact> {
    match node {
        GraphNode::Artifact(artifact) => Some(ScannableArtifact {
            id: artifact.id.clone(),
            path: artifact.path.clone(),
            display_path: artifact.display_path.clone(),
            resolve_from_dir: Path::new(&artifact.path)
                .parent()
                .unwrap_or_else(|| Path::new(&artifact.path))
                .to_path_buf(),
            artifact_type: artifact.artifact_type.clone(),
            tool_family: artifact.tool_family.clone(),
        }),
        GraphNode::PluginArtifact(artifact) => Some(ScannableArtifact {
            id: artifact.id.clone(),
            path: artifact.path.clone(),
            display_path: artifact.display_path.clone(),
            resolve_from_dir: artifact
                .resolve_from_path
                .as_deref()
                .map(PathBuf::from)
                .or_else(|| {
                    Path::new(&artifact.path)
                        .parent()
                        .map(Path::to_path_buf)
                })
                .unwrap_or_else(|| PathBuf::from(&artifact.path)),
            artifact_type: artifact.artifact_type.clone(),
            tool_family: "plugin".to_string(),
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
fn discover_project_candidates(
    roots: &[PathBuf],
    known_global_dirs: &[PathBuf],
    max_depth: usize,
    catalogs: &HashMap<String, ToolCatalog>,
) -> Vec<ProjectCandidate> {
    discover_project_candidates_with_progress(
        roots,
        known_global_dirs,
        max_depth,
        catalogs,
        Path::new(""),
        &mut |_| Ok(()),
    )
    .unwrap_or_default()
}

fn discover_project_candidates_with_progress(
    roots: &[PathBuf],
    known_global_dirs: &[PathBuf],
    max_depth: usize,
    catalogs: &HashMap<String, ToolCatalog>,
    home_dir: &Path,
    on_progress: &mut dyn FnMut(ScanProgress) -> Result<()>,
) -> Result<Vec<ProjectCandidate>> {
    let mut search_roots = roots.iter().cloned().collect::<Vec<_>>();
    search_roots.extend(known_global_dirs.iter().cloned());
    search_roots.sort();
    search_roots.dedup();
    let compiled_rules = compile_project_discovery_rules(catalogs)?;

    let mut signals = Vec::new();
    for root in &search_roots {
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
            signals.push(CandidateSignal {
                root_path: root.clone(),
                kind: ProjectKind::GitRepo,
                score: 300,
                reason: "Directory contains .git.".to_string(),
            });
        }

        for entry in WalkDir::new(root)
            .max_depth(max_depth)
            .into_iter()
            .filter_entry(|entry| should_traverse_candidate_entry(entry))
            .filter_map(Result::ok)
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

            let path = entry.path();
            if entry.file_type().is_dir() && path.join(".git").exists() {
                signals.push(CandidateSignal {
                    root_path: path.to_path_buf(),
                    kind: ProjectKind::GitRepo,
                    score: 300,
                    reason: "Directory contains .git.".to_string(),
                });
            }

            let relative = path.strip_prefix(root).unwrap_or(path);
            for signal in project_discovery_signals_for_entry(path, relative, root, &compiled_rules) {
                signals.push(signal);
            }
        }
    }

    Ok(finalize_project_candidates(signals))
}

fn should_traverse_candidate_entry(entry: &walkdir::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    !matches!(name.as_ref(), ".git" | "node_modules" | "target" | "dist")
}

fn compile_project_discovery_rules(
    catalogs: &HashMap<String, ToolCatalog>,
) -> Result<Vec<CompiledProjectDiscoveryRule>> {
    let mut compiled = Vec::new();
    for catalog in catalogs.values() {
        for ProjectDiscoveryRule {
            glob,
            kind,
            score,
            reason,
            root_strategy,
            skip_if_scan_root,
        } in &catalog.project_discovery_rules
        {
            compiled.push(CompiledProjectDiscoveryRule {
                kind: kind.clone(),
                score: *score,
                reason: reason.clone(),
                root_strategy: root_strategy.clone(),
                skip_if_scan_root: *skip_if_scan_root,
                matcher: Glob::new(glob)?.compile_matcher(),
            });
        }
    }
    Ok(compiled)
}

fn project_discovery_signals_for_entry(
    path: &Path,
    relative: &Path,
    scan_root: &Path,
    rules: &[CompiledProjectDiscoveryRule],
) -> Vec<CandidateSignal> {
    if !path.is_file() {
        return Vec::new();
    }

    let mut signals = Vec::new();
    for rule in rules {
        if !project_discovery_rule_matches(relative, scan_root, &rule.matcher) {
            continue;
        }
        let Some(root_path) = resolve_project_discovery_root(path, scan_root, &rule.root_strategy)
        else {
            continue;
        };
        if rule.skip_if_scan_root && !root_path.starts_with(scan_root) {
            continue;
        }
        signals.push(CandidateSignal {
            root_path,
            kind: rule.kind.clone(),
            score: rule.score,
            reason: rule.reason.clone(),
        });
    }
    signals
}

fn project_discovery_rule_matches(
    relative: &Path,
    scan_root: &Path,
    matcher: &globset::GlobMatcher,
) -> bool {
    project_discovery_match_candidates(relative, scan_root)
        .into_iter()
        .any(|candidate| matcher.is_match(candidate))
}

fn project_discovery_match_candidates(relative: &Path, scan_root: &Path) -> Vec<String> {
    let components = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Vec::new();
    }

    let mut matches = Vec::new();
    for index in 0..components.len() {
        matches.push(components[index..].join("/"));
    }

    if let Some(root_name) = scan_root.file_name().and_then(|name| name.to_str()) {
        if !root_name.is_empty() {
            let prefixed = matches
                .iter()
                .map(|candidate| format!("{root_name}/{candidate}"))
                .collect::<Vec<_>>();
            matches.extend(prefixed);
        }
    }

    matches.sort();
    matches.dedup();
    matches
}

fn resolve_project_discovery_root(
    path: &Path,
    scan_root: &Path,
    strategy: &ProjectDiscoveryRootStrategy,
) -> Option<PathBuf> {
    match strategy {
        ProjectDiscoveryRootStrategy::MatchParent => path.parent().map(Path::to_path_buf),
        ProjectDiscoveryRootStrategy::LevelsUp { count } => {
            let mut current = if is_hidden_plugin_manifest(path) {
                path.parent()?
            } else {
                path
            };
            for _ in 0..*count {
                current = current.parent()?;
            }
            Some(current.to_path_buf())
        }
        ProjectDiscoveryRootStrategy::NearestPluginRoot => {
            Some(nearest_plugin_root(path.parent()?, scan_root))
        }
    }
}

fn is_hidden_plugin_manifest(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("plugin.json")
        && matches!(
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str()),
            Some(".codex-plugin" | ".claude-plugin")
        )
}

fn nearest_plugin_root(start: &Path, scan_root: &Path) -> PathBuf {
    let mut current = Some(start);
    while let Some(path) = current {
        if path.join(".codex-plugin").join("plugin.json").exists()
            || path.join(".claude-plugin").join("plugin.json").exists()
            || path.join(".mcp.json").exists()
        {
            return path.to_path_buf();
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("skills") {
            return path.parent().unwrap_or(path).to_path_buf();
        }
        if path == scan_root {
            break;
        }
        current = path.parent();
    }
    start.to_path_buf()
}

fn finalize_project_candidates(signals: Vec<CandidateSignal>) -> Vec<ProjectCandidate> {
    let mut merged = HashMap::<(PathBuf, ProjectKind), ProjectCandidate>::new();
    for signal in signals {
        let key = (signal.root_path.clone(), signal.kind.clone());
        let name = signal
            .root_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();
        merged
            .entry(key)
            .and_modify(|candidate| {
                let prior_score = candidate.signal_score;
                candidate.signal_score = candidate.signal_score.max(signal.score);
                if candidate.discovery_reason.is_empty() || signal.score > prior_score {
                    candidate.discovery_reason = signal.reason.clone();
                }
            })
            .or_insert(ProjectCandidate {
                root_path: signal.root_path,
                name,
                kind: signal.kind,
                discovery_reason: signal.reason,
                signal_score: signal.score,
            });
    }

    let mut candidates = merged.into_values().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        project_kind_rank(&left.kind)
            .cmp(&project_kind_rank(&right.kind))
            .then(right.signal_score.cmp(&left.signal_score))
            .then(left.root_path.cmp(&right.root_path))
    });

    let mut kept = Vec::<ProjectCandidate>::new();
    'candidate: for candidate in candidates {
        for existing in &kept {
            if candidate.root_path == existing.root_path {
                if project_kind_rank(&candidate.kind) >= project_kind_rank(&existing.kind) {
                    continue 'candidate;
                }
            }
            if candidate.root_path.starts_with(&existing.root_path) {
                if matches!(existing.kind, ProjectKind::GitRepo) {
                    continue 'candidate;
                }
                if candidate.kind == existing.kind {
                    continue 'candidate;
                }
            }
        }
        kept.push(candidate);
    }

    kept.sort_by(|left, right| {
        project_kind_rank(&left.kind)
            .cmp(&project_kind_rank(&right.kind))
            .then(right.signal_score.cmp(&left.signal_score))
            .then(left.name.cmp(&right.name))
            .then(left.root_path.cmp(&right.root_path))
    });
    kept
}

fn project_kind_rank(kind: &ProjectKind) -> i32 {
    match kind {
        ProjectKind::GitRepo => 0,
        ProjectKind::WorkspaceCandidate => 1,
        ProjectKind::PluginPackage => 2,
    }
}

fn project_kind_label(kind: &ProjectKind) -> &'static str {
    match kind {
        ProjectKind::GitRepo => "repo",
        ProjectKind::WorkspaceCandidate => "workspace",
        ProjectKind::PluginPackage => "plugin package",
    }
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
        domain::{GraphNode, NodeState, ProjectKind, ProjectSummary},
        storage::Store,
    };

    use super::{
        build_surface_state, collect_repo_files, display_path, reindex_project_tool_with_progress,
        scan_projects, scan_projects_with_progress, stable_id, EdgeType,
    };

    fn demo_project_summary(root: &std::path::Path, home: &std::path::Path) -> ProjectSummary {
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
        assert!(progress.iter().any(|update| update.phase == "root"));
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
    fn codex_directory_references_expand_to_descendant_files() {
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
