use std::{
    collections::{BTreeMap, HashMap, HashSet},
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
    services::refs::extract_references,
    storage::Store,
};

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
    let repo_roots = discover_repos(&roots, config.scan_max_depth);

    let mut summaries = Vec::new();
    for repo_root in repo_roots {
        let summary = scan_project(config, store, &catalogs, &repo_root)?;
        summaries.push(summary);
    }
    store.write_json(&store.projects_index_path(), &summaries)?;
    Ok(summaries)
}

pub fn load_surface_state(store: &Store, project_id: &str, tool: &str) -> Result<SurfaceState> {
    store.read_json(&store.tool_state_path(project_id, tool))
}

fn scan_project(
    config: &AppConfig,
    store: &Store,
    catalogs: &HashMap<String, ToolCatalog>,
    repo_root: &Path,
) -> Result<ProjectSummary> {
    let indexed_at = Utc::now();
    let project_id = stable_id("project", &repo_root.to_string_lossy());
    let summary = ProjectSummary {
        id: project_id.clone(),
        root_path: repo_root.to_string_lossy().to_string(),
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
    let inventory = collect_repo_files(repo_root, config.scan_max_depth);
    store.write_json(&store.inventory_path(&project_id), &inventory)?;

    let mut union_nodes = BTreeMap::<String, GraphNode>::new();
    let mut union_edges = Vec::<GraphEdge>::new();

    for catalog in catalogs.values() {
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

    for artifact in collect_artifacts_from_rules(&repo_root, inventory, catalog, indexed_at)? {
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
                why_included: vec![format!("Detected in global scope: {}", artifact.path)],
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

    let reference_edges = collect_reference_edges(&mut nodes, &tool_node, config, indexed_at)?;
    edges.extend(reference_edges);

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

    for install_root in &plugin_system.install_roots {
        let root = resolve_catalog_path(install_root, &config.home_dir, Some(repo_root));
        if !root.exists() {
            continue;
        }
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let plugin_root = entry.path();
            if !plugin_root.is_dir() {
                continue;
            }

            let manifest = plugin_system
                .manifest_paths
                .iter()
                .map(|rel| plugin_root.join(rel))
                .find(|path| path.exists());
            let plugin_name = manifest
                .as_ref()
                .and_then(|path| read_plugin_name(path))
                .or_else(|| {
                    plugin_root
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(ToString::to_string)
                })
                .unwrap_or_else(|| "unknown-plugin".to_string());
            let disabled = plugin_disabled(&plugin_name, &config_paths);
            let mut states = vec![NodeState::Installed, NodeState::Configured];
            let reason = if disabled {
                states.push(NodeState::Inactive);
                "Plugin installed locally but disabled in config.".to_string()
            } else {
                states.push(NodeState::Effective);
                "Plugin installed locally and not disabled in config.".to_string()
            };
            let plugin_id = stable_id(
                "plugin",
                &format!("{}:{}", plugin_system.system, plugin_root.display()),
            );
            let plugin_node = GraphNode::Plugin(PluginNode {
                id: plugin_id.clone(),
                name: plugin_name.clone(),
                plugin_system: plugin_system.system.clone(),
                install_root: plugin_root.to_string_lossy().to_string(),
                manifest_path: manifest
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                states: states.clone(),
                confidence: if disabled { 0.88 } else { 0.94 },
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
            if disabled {
                edges.push(GraphEdge {
                    from: plugin_id.clone(),
                    to: tool_node.id().to_string(),
                    edge_type: EdgeType::Disables,
                    hardness: "hard".to_string(),
                    reason: "Config explicitly disables plugin.".to_string(),
                });
            } else {
                edges.push(GraphEdge {
                    from: plugin_id.clone(),
                    to: tool_node.id().to_string(),
                    edge_type: EdgeType::Enables,
                    hardness: "hard".to_string(),
                    reason: "Config leaves plugin enabled.".to_string(),
                });
            }
            for compatibility in &plugin_system.compatibility {
                edges.push(GraphEdge {
                    from: plugin_id.clone(),
                    to: format!("tool:{compatibility}"),
                    edge_type: EdgeType::CompatibleWith,
                    hardness: "soft".to_string(),
                    reason: "Catalog declares compatibility.".to_string(),
                });
            }

            if let Some(manifest) = &manifest {
                let artifact_id = stable_id("plugin_artifact", &manifest.to_string_lossy());
                nodes.push(GraphNode::PluginArtifact(PluginArtifactNode {
                    id: artifact_id.clone(),
                    plugin_id: plugin_id.clone(),
                    path: manifest.to_string_lossy().to_string(),
                    artifact_type: ArtifactType::PluginManifest,
                    states: vec![NodeState::Declared, NodeState::Effective],
                    confidence: 0.95,
                    reason: "Plugin manifest detected.".to_string(),
                }));
                edges.push(GraphEdge {
                    from: plugin_id.clone(),
                    to: artifact_id.clone(),
                    edge_type: EdgeType::ProvidesArtifact,
                    hardness: "hard".to_string(),
                    reason: "Plugin manifest belongs to plugin.".to_string(),
                });
            }
            let readme = plugin_root.join("README.md");
            if readme.exists() {
                let artifact_id = stable_id("plugin_artifact", &readme.to_string_lossy());
                nodes.push(GraphNode::PluginArtifact(PluginArtifactNode {
                    id: artifact_id.clone(),
                    plugin_id: plugin_id.clone(),
                    path: readme.to_string_lossy().to_string(),
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
    }
    Ok((nodes, edges, verdicts))
}

fn collect_reference_edges(
    nodes: &mut BTreeMap<String, GraphNode>,
    tool_node: &GraphNode,
    config: &AppConfig,
    indexed_at: DateTime<Utc>,
) -> Result<Vec<GraphEdge>> {
    let mut edges = Vec::new();
    let artifact_nodes = nodes
        .values()
        .filter_map(|node| match node {
            GraphNode::Artifact(artifact) => Some(artifact.clone()),
            GraphNode::PluginArtifact(artifact) => Some(ArtifactNode {
                id: artifact.id.clone(),
                path: artifact.path.clone(),
                artifact_type: artifact.artifact_type.clone(),
                tool_family: "plugin".to_string(),
                scope_type: ScopeType::PluginProvided,
                states: artifact.states.clone(),
                confidence: artifact.confidence,
                origin: "plugin".to_string(),
                last_indexed_at: indexed_at,
                hash: file_hash(Path::new(&artifact.path))
                    .unwrap_or_else(|_| "missing".to_string()),
                mtime: None,
                reason: artifact.reason.clone(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();

    for artifact in artifact_nodes {
        let path = Path::new(&artifact.path);
        if !path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for hit in extract_references(path, &content, &config.home_dir) {
            let target_id = stable_id("reference", &hit.resolved_path.to_string_lossy());
            if !nodes.contains_key(&target_id) {
                nodes.insert(
                    target_id.clone(),
                    GraphNode::Artifact(ArtifactNode {
                        id: target_id.clone(),
                        path: hit.resolved_path.to_string_lossy().to_string(),
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
                        confidence: if hit.broken { 0.85 } else { 0.65 },
                        origin: "reference".to_string(),
                        last_indexed_at: indexed_at,
                        hash: file_hash(&hit.resolved_path)
                            .unwrap_or_else(|_| "missing".to_string()),
                        mtime: fs::metadata(&hit.resolved_path).ok().and_then(mtime_utc),
                        reason: if hit.broken {
                            format!("Referenced path missing: {}", hit.raw)
                        } else {
                            format!("Referenced from {}", artifact.path)
                        },
                    }),
                );
            }
            edges.push(GraphEdge {
                from: artifact.id.clone(),
                to: target_id.clone(),
                edge_type: hit.edge_type,
                hardness: if hit.broken { "soft" } else { "hard" }.to_string(),
                reason: if hit.broken {
                    format!("Broken reference found in {}.", artifact.path)
                } else {
                    format!("Reference found in {}.", artifact.path)
                },
            });
            edges.push(GraphEdge {
                from: tool_node.id().to_string(),
                to: target_id,
                edge_type: EdgeType::References,
                hardness: "soft".to_string(),
                reason: "Tool context reaches reference target through artifact graph.".to_string(),
            });
        }
    }
    Ok(edges)
}

fn collect_repo_files(root: &Path, max_depth: usize) -> Vec<String> {
    WalkDir::new(root)
        .max_depth(max_depth)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path() != root)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy();
            !matches!(name.as_ref(), ".git" | "node_modules" | "target" | "dist")
        })
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            entry
                .path()
                .strip_prefix(root)
                .ok()
                .map(|path| path.to_path_buf())
        })
        .map(|path| path.to_string_lossy().to_string())
        .collect()
}

fn discover_repos(roots: &[PathBuf], max_depth: usize) -> Vec<PathBuf> {
    let mut repos = HashSet::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
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
            if entry.path().join(".git").exists() {
                repos.insert(entry.path().to_path_buf());
            }
        }
    }
    let mut repos = repos.into_iter().collect::<Vec<_>>();
    repos.sort();
    repos
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
                .and_then(|value| {
                    value
                        .get("plugins")
                        .and_then(|plugins| plugins.get(plugin_name))
                        .and_then(|entry| entry.get("enabled"))
                        .and_then(|flag| flag.as_bool())
                })
                .is_some_and(|enabled| !enabled),
            Some("json") => serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|value| {
                    value
                        .get("plugins")
                        .and_then(|plugins| plugins.get(plugin_name))
                        .and_then(|entry| entry.get("enabled"))
                        .and_then(|flag| flag.as_bool())
                })
                .is_some_and(|enabled| !enabled),
            _ => false,
        }
    })
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

    use super::{build_surface_state, collect_repo_files, scan_projects};

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
            .any(|edge| edge.reason.contains("Reference found")));
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
}
