use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use walkdir::WalkDir;

use crate::{
    config::AppConfig,
    domain::{
        ArtifactNode, ArtifactType, EdgeType, GraphEdge, GraphNode, NodeState,
        ScopeType, Verdict,
    },
    services::refs::{extract_metadata, extract_references, ResolverContext},
    services::projects::discovery::display_path,
};

use crate::services::graph::{
    stable_id, file_hash, mtime_utc, confidence_from_states, node_verdict, upsert_verdict,
};

#[derive(Clone, Debug)]
pub struct PromotableEdge {
    pub from: String,
    pub to: String,
    pub reason: String,
}

#[derive(Default)]
pub struct ReferenceCollection {
    pub edges: Vec<GraphEdge>,
    pub verdicts: Vec<Verdict>,
    pub promotable_edges: Vec<PromotableEdge>,
}

#[derive(Clone, Debug)]
pub struct ScannableArtifact {
    pub id: String,
    pub path: String,
    pub display_path: String,
    pub resolve_from_dir: PathBuf,
    pub artifact_type: ArtifactType,
    pub tool_family: String,
}

pub fn collect_reference_edges(
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

        if let Some(meta) = extract_metadata(&context, &content) {
            if let Some(GraphNode::Artifact(ref mut n)) = nodes.get_mut(&artifact.id) {
                n.metadata = Some(meta);
            }
        }

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
                        byte_size: fs::metadata(&hit.resolved_path).map(|m| m.len()).unwrap_or(0),
                        reason: target_reason.clone(),
                        metadata: None,
                        health: None,
                    }),
                );
                existing_path_to_id.insert(target_path.clone(), target_id.clone());
                collection.verdicts.push(node_verdict(
                    &target_id,
                    if hit.broken {
                        &[NodeState::BrokenReference, NodeState::Unresolved] as &[_]
                    } else {
                        &[NodeState::ReferencedOnly] as &[_]
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
                if matches!(artifact.artifact_type, ArtifactType::PluginManifest) {
                    link_existing_directory_descendants(
                        &target_id,
                        &hit.resolved_path,
                        &target_display_path,
                        nodes,
                        &mut collection,
                    );
                } else {
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
    }
    Ok(collection)
}

fn link_existing_directory_descendants(
    directory_id: &str,
    directory_path: &Path,
    directory_display_path: &str,
    nodes: &BTreeMap<String, GraphNode>,
    collection: &mut ReferenceCollection,
) {
    let mut descendants = nodes
        .values()
        .filter_map(artifact_node_from_graph)
        .filter(|artifact| {
            let artifact_path = Path::new(&artifact.path);
            artifact_path.is_file() && artifact_path.starts_with(directory_path)
        })
        .map(|artifact| artifact.id)
        .collect::<Vec<_>>();
    descendants.sort();
    descendants.dedup();

    for descendant_id in descendants {
        let reason = format!(
            "Contained in referenced directory: {}.",
            directory_display_path
        );
        collection.edges.push(GraphEdge {
            from: directory_id.to_string(),
            to: descendant_id.clone(),
            edge_type: EdgeType::References,
            hardness: "hard".to_string(),
            reason: reason.clone(),
        });
        collection.promotable_edges.push(PromotableEdge {
            from: directory_id.to_string(),
            to: descendant_id,
            reason,
        });
    }
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
                    byte_size: fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0),
                    reason: reason.clone(),
                    metadata: None,
                    health: None,
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

pub fn promote_effective_closure(
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

pub fn dedupe_edges(edges: Vec<GraphEdge>) -> Vec<GraphEdge> {
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
