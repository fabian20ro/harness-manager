use std::{
    collections::{HashMap},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    catalogs::{catalog_path, seed_catalogs},
    config::AppConfig,
    domain::{ProjectSummary, SurfaceState, ToolCatalog},
    storage::Store,
};

pub use crate::services::graph::ScanRunContext;
use crate::services::graph::{
    build_surface_state_with_context, rewrite_project_union_graph, stable_id,
};
use crate::services::projects::discovery::{
    discover_project_candidates_with_progress, display_path, ProjectCandidate,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
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
    let mut scan_run = ScanRunContext::default();
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

    // Adapter for progress
    let mut on_progress_adapter = |p: crate::services::projects::discovery::ScanProgress| {
        on_progress(ScanProgress {
            phase: p.phase,
            message: p.message,
            current_path: p.current_path,
            items_done: p.items_done,
            items_total: p.items_total,
        })
    };

    let project_candidates = discover_project_candidates_with_progress(
        &roots,
        &config.known_global_dirs,
        config.scan_max_depth,
        &catalogs,
        &config.home_dir,
        &mut on_progress_adapter,
    )?;

    let mut summaries = Vec::new();
    let total_projects = project_candidates.len();
    for (index, candidate) in project_candidates.iter().enumerate() {
        let project_display_path = display_path(&candidate.root_path, &config.home_dir);
        on_progress(ScanProgress {
            phase: "repo".to_string(),
            message: format!("Indexing {}", project_display_path),
            current_path: Some(project_display_path.clone()),
            items_done: Some(index),
            items_total: Some(total_projects),
        })?;
        let summary = scan_project(
            config,
            store,
            &catalogs,
            candidate,
            &mut scan_run,
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
    let mut scan_run = ScanRunContext::default();

    let state = build_surface_state_with_context(
        config,
        store,
        &project,
        catalog,
        &inventory,
        &mut scan_run,
        &mut |_| Ok(()),
    )?;
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
    let mut scan_run = ScanRunContext::default();
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

    let state = build_surface_state_with_context(
        config,
        store,
        &updated_project,
        catalog,
        &inventory,
        &mut scan_run,
        &mut on_progress,
    )?;
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
    scan_run: &mut ScanRunContext,
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
        let state = build_surface_state_with_context(
            config,
            store,
            &summary,
            catalog,
            &inventory,
            scan_run,
            on_progress,
        )?;
        store.write_json(
            &store.tool_state_path(&project_id, &catalog.surface),
            &state,
        )?;
    }

    rewrite_project_union_graph(store, &project_id)?;

    Ok(summary)
}

#[allow(dead_code)]
pub(crate) fn collect_repo_files(root: &Path, max_depth: usize) -> Vec<String> {
    collect_repo_files_with_progress(root, max_depth, Path::new(""), &mut |_| Ok(()))
        .unwrap_or_default()
}

pub(crate) fn collect_repo_files_with_progress(
    root: &Path,
    max_depth: usize,
    home_dir: &Path,
    on_progress: &mut dyn FnMut(String) -> Result<()>,
) -> Result<Vec<String>> {
    use walkdir::WalkDir;
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

#[cfg(test)]
pub(crate) fn build_surface_state_for_test(
    config: &AppConfig,
    store: &Store,
    project: &crate::domain::ProjectSummary,
    catalog: &crate::domain::ToolCatalog,
    inventory: &[String],
) -> Result<crate::domain::SurfaceState> {
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

#[cfg(test)]
pub(crate) fn collect_repo_files_for_test(root: &Path, max_depth: usize) -> Vec<String> {
    collect_repo_files(root, max_depth)
}

#[cfg(test)]
pub(crate) fn display_path_for_test(path: &Path, home_dir: &Path) -> String {
    display_path(path, home_dir)
}
