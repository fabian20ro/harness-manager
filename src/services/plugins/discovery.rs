use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Result};
use walkdir::WalkDir;

use crate::{
    config::AppConfig,
    domain::PluginSystemCatalog,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PluginDiscoveryCacheKey {
    pub system: String,
    pub roots: Vec<(PathBuf, String)>,
    pub manifest_paths: Vec<String>,
    pub max_depth: usize,
}

#[derive(Clone, Debug)]
pub struct PluginCandidate {
    pub key: String,
    pub name: String,
    pub install_root: PathBuf,
    pub manifest_path: Option<PathBuf>,
    pub manifest_base_dir: Option<PathBuf>,
    pub readme_path: Option<PathBuf>,
    pub discovery_sources: Vec<String>,
}

#[derive(Default)]
pub struct PluginDiscoveryCache {
    pub cache: HashMap<PluginDiscoveryCacheKey, Vec<PluginCandidate>>,
    #[cfg(test)]
    pub call_counts: HashMap<String, usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanProgress {
    pub phase: String,
    pub message: String,
    pub current_path: Option<String>,
    pub items_done: Option<usize>,
    pub items_total: Option<usize>,
}

pub fn discover_plugins_with_cache(
    discovery_cache: &mut PluginDiscoveryCache,
    config: &AppConfig,
    plugin_system: &PluginSystemCatalog,
    repo_root: &Path,
    repo_display_path: &str,
    on_progress: &mut dyn FnMut(ScanProgress) -> Result<()>,
) -> Result<Vec<PluginCandidate>> {
    let (roots, max_depth) = plugin_discovery_roots(config, plugin_system, repo_root);
    let cache_key = PluginDiscoveryCacheKey {
        system: plugin_system.system.clone(),
        roots: roots
            .iter()
            .map(|(path, source)| (path.clone(), (*source).to_string()))
            .collect(),
        manifest_paths: plugin_system.manifest_paths.clone(),
        max_depth,
    };
    let system_name = plugin_system_display_name(&plugin_system.system);

    if let Some(cached) = discovery_cache.cache.get(&cache_key) {
        on_progress(ScanProgress {
            phase: "surface".to_string(),
            message: format!("Reusing cached {system_name} plugin discovery for {repo_display_path}"),
            current_path: Some(repo_display_path.to_string()),
            items_done: None,
            items_total: None,
        })?;
        return Ok(cached.clone());
    }

    on_progress(ScanProgress {
        phase: "surface".to_string(),
        message: format!("Discovering {system_name} plugins for {repo_display_path}"),
        current_path: Some(repo_display_path.to_string()),
        items_done: None,
        items_total: None,
    })?;
    let candidates = discover_plugins_from_roots(&roots, &plugin_system.manifest_paths, max_depth)?;
    #[cfg(test)]
    {
        *discovery_cache
            .call_counts
            .entry(plugin_system.system.clone())
            .or_insert(0) += 1;
    }
    discovery_cache
        .cache
        .insert(cache_key, candidates.clone());
    Ok(candidates)
}

fn plugin_discovery_roots(
    config: &AppConfig,
    plugin_system: &PluginSystemCatalog,
    repo_root: &Path,
) -> (Vec<(PathBuf, &'static str)>, usize) {
    let mut roots = plugin_system
        .install_roots
        .iter()
        .map(|path| resolve_catalog_path(path, &config.home_dir, Some(repo_root)))
        .map(|path| (path, "install_root"))
        .collect::<Vec<_>>();

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
            roots.extend(installed_roots);
            roots.push((
                config.home_dir.join(".claude").join("plugins").join("marketplaces"),
                "marketplace_layout",
            ));
            roots.push((
                config.home_dir.join(".claude").join("plugins").join("cache"),
                "cache_layout",
            ));
        }
        _ => {}
    }

    let max_depth = match plugin_system.system.as_str() {
        "claude" => config.scan_max_depth + 5,
        _ => config.scan_max_depth + 4,
    };
    (roots, max_depth)
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

fn discover_plugins_from_roots(
    roots: &[(PathBuf, &'static str)],
    manifest_paths: &[String],
    max_depth: usize,
) -> Result<Vec<PluginCandidate>> {
    let mut candidates = HashMap::<String, PluginCandidate>::new();
    for candidate in discover_plugin_candidates(roots, manifest_paths, max_depth) {
        merge_plugin_candidate(&mut candidates, candidate);
    }
    let mut values = candidates.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(values)
}

fn discover_plugin_candidates(
    roots: &[(PathBuf, &str)],
    manifest_paths: &[String],
    max_depth: usize,
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
                        );
                        let key = (candidate.install_root.clone(), entry.path().to_path_buf());
                        if let Some(existing) = discovered.get_mut(&key) {
                            for source in candidate.discovery_sources {
                                if !existing.discovery_sources.contains(&source) {
                                    existing.discovery_sources.push(source);
                                }
                            }
                        } else {
                            discovered.insert(key, candidate);
                        }
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
        for source in candidate.discovery_sources {
            if !existing.discovery_sources.contains(&source) {
                existing.discovery_sources.push(source);
            }
        }
        return;
    }
    candidates.insert(key, candidate);
}

fn read_plugin_name(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        let value = serde_json::from_str::<serde_json::Value>(&text).ok()?;
        return value
            .get("name")
            .and_then(|v| v.as_str())
            .map(ToString::to_string);
    }
    None
}

fn plugin_system_display_name(system: &str) -> &str {
    match system {
        "codex" => "Codex",
        "claude" => "Claude",
        "gemini" => "Gemini",
        "pi" => "Pi",
        "opencode" => "OpenCode",
        _ => system,
    }
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
