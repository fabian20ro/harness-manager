use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSetBuilder};

use crate::{
    config::AppConfig,
    domain::{
        ArtifactNode, ArtifactRule, EdgeType, GraphEdge, GraphNode, NodeState,
        ProjectSummary, RemoteSnapshotNode, ScopeType,
        SnapshotAssociation, SurfaceState, ToolCatalog, ToolContext, ToolContextNode, Verdict,
    },
    storage::Store,
    services::plugins::discovery::{PluginDiscoveryCache},
    services::projects::discovery::display_path,
};

use super::scan::ScanProgress;

pub mod util;
pub mod metadata;
pub mod plugins;
pub mod edges;

pub use util::{stable_id, file_hash, mtime_utc, confidence_from_states, resolve_catalog_path};
pub use edges::{collect_reference_edges, promote_effective_closure, dedupe_edges, ReferenceCollection, ScannableArtifact, PromotableEdge};
pub use plugins::{collect_plugins, discover_codex_skill_artifacts};

#[derive(Default)]
pub struct ScanRunContext {
    pub plugin_discovery_cache: PluginDiscoveryCache,
}

pub fn build_surface_state_with_context(
    config: &AppConfig,
    store: &Store,
    project: &ProjectSummary,
    catalog: &ToolCatalog,
    inventory: &[String],
    scan_run: &mut ScanRunContext,
    on_progress: &mut dyn FnMut(ScanProgress) -> Result<()>,
) -> Result<SurfaceState> {
    let mut nodes = BTreeMap::<String, GraphNode>::new();
    let mut edges = Vec::<GraphEdge>::new();
    let mut verdicts = Vec::<Verdict>::new();
    let indexed_at = Utc::now();

    let project_node = GraphNode::Project(crate::domain::ProjectNode {
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
            &project.display_path,
            indexed_at,
            scan_run,
            on_progress,
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

pub fn rewrite_project_union_graph(store: &Store, project_id: &str) -> Result<()> {
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
                    mtime: metadata.as_ref().and_then(|m| mtime_utc(m.clone())),
                    byte_size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                    reason: reason.clone(),
                    metadata: None,
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
            mtime: metadata.as_ref().and_then(|m| mtime_utc(m.clone())),
            byte_size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
            reason: location.reason.clone(),
            metadata: None,
        });
    }
    Ok(nodes)
}

pub fn node_verdict(entity_id: &str, states: &[NodeState], reason: &str) -> Verdict {
    Verdict {
        entity_id: entity_id.to_string(),
        states: states.to_vec(),
        why_included: vec![reason.to_string()],
        why_excluded: Vec::new(),
        shadowed_by: Vec::new(),
        provenance_paths: vec![vec![entity_id.to_string()]],
    }
}

pub fn upsert_verdict<'a>(
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
