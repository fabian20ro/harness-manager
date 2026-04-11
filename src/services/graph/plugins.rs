use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use walkdir::WalkDir;

use crate::{
    config::AppConfig,
    domain::{
        ArtifactType, EdgeType, GraphEdge, GraphNode, NodeState,
        ToolCatalog, Verdict, PluginArtifactNode,
    },
    services::plugins::discovery::{discover_plugins_with_cache},
    services::projects::discovery::display_path,
};

use crate::services::scan::ScanProgress;
use crate::services::graph::{ScanRunContext, node_verdict, stable_id, resolve_catalog_path};

pub fn collect_plugins(
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
                health: None,
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
                health: None,
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

pub fn discover_codex_skill_artifacts(
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
                health: None,
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
    use super::metadata::parse_skill_metadata;
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
        health: None,
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

pub fn plugin_disabled(plugin_name: &str, config_paths: &[PathBuf]) -> bool {
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
