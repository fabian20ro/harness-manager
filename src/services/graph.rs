use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSetBuilder};
use sha2::{Digest, Sha256};
use serde_json::{Map as JsonMap, Value as JsonValue};

use crate::{
    config::AppConfig,
    domain::{
        ArtifactNode, ArtifactRule, ArtifactType, EdgeType, GraphEdge, GraphNode, NodeState,
        ProjectSummary, RemoteSnapshotNode, ScopeType,
        SnapshotAssociation, SurfaceState, ToolCatalog, ToolContext, ToolContextNode, Verdict,
        PluginArtifactNode,
    },
    services::refs::{extract_metadata, extract_references, ResolverContext},
    storage::Store,
    services::plugins::discovery::{discover_plugins_with_cache, PluginDiscoveryCache},
    services::projects::discovery::display_path,
};

use super::scan::ScanProgress;

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

#[derive(Default)]
pub struct ScanRunContext {
    pub plugin_discovery_cache: PluginDiscoveryCache,
}

#[derive(Clone, Debug, Default)]
struct ParsedSkillMetadata {
    name: Option<String>,
    description: Option<String>,
    metadata: Option<JsonValue>,
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

fn resolve_catalog_path(pattern: &str, home_dir: &Path, project_root: Option<&Path>) -> PathBuf {
    let with_home = pattern.replace('~', &home_dir.to_string_lossy());
    let with_project = if let Some(project_root) = project_root {
        with_home.replace("{project}", &project_root.to_string_lossy())
    } else {
        with_home
    };
    PathBuf::from(with_project)
}

fn collect_plugins(
    config: &AppConfig,
    catalog: &ToolCatalog,
    plugin_system: &crate::domain::PluginSystemCatalog,
    tool_node: &GraphNode,
    repo_root: &Path,
    repo_display_path: &str,
    _indexed_at: DateTime<Utc>,
    scan_run: &mut ScanRunContext,
    on_progress: &mut dyn FnMut(ScanProgress) -> Result<()>,
) -> Result<(Vec<GraphNode>, Vec<GraphEdge>, Vec<Verdict>)> {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut verdicts = Vec::new();
    let config_paths = plugin_system
        .config_paths
        .iter()
        .map(|path| resolve_catalog_path(path, &config.home_dir, Some(repo_root)))
        .collect::<Vec<_>>();

    // We need to map our ScanProgress to plugins::discovery::ScanProgress
    let mut on_progress_adapter = |p: crate::services::plugins::discovery::ScanProgress| {
        on_progress(ScanProgress {
            phase: p.phase,
            message: p.message,
            current_path: p.current_path,
            items_done: p.items_done,
            items_total: p.items_total,
        })
    };

    let candidates = discover_plugins_with_cache(
        &mut scan_run.plugin_discovery_cache,
        config,
        plugin_system,
        repo_root,
        repo_display_path,
        &mut on_progress_adapter,
    )?;

    for candidate in candidates {
        let disabled = plugin_disabled(&candidate.name, &config_paths);
        let mut states = vec![NodeState::Installed, NodeState::Configured];
        let reason = if disabled {
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
        let plugin_node = GraphNode::Plugin(crate::domain::PluginNode {
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
        edges.push(GraphEdge {
            from: plugin_id.clone(),
            to: tool_node.id().to_string(),
            edge_type: if disabled {
                EdgeType::Disables
            } else {
                EdgeType::Enables
            },
            hardness: "hard".to_string(),
            reason: if disabled {
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
                    disabled,
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

pub fn stable_id(prefix: &str, input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{prefix}:{:x}", hasher.finalize())
}

pub fn file_hash(path: &Path) -> Result<String> {
    if path.is_dir() {
        return Ok("directory".to_string());
    }
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn mtime_utc(metadata: fs::Metadata) -> Option<DateTime<Utc>> {
    metadata.modified().ok().map(DateTime::<Utc>::from)
}

pub fn confidence_from_states(states: &[NodeState]) -> f32 {
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

use walkdir::WalkDir;
