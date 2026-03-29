use std::{fs, path::Path};

use anyhow::{Context, Result, anyhow};

use crate::{
    config::AppConfig,
    domain::{EditBackup, EditableMetadata, GraphNode, InspectPayload, SaveInspectResponse},
    services::scan::{load_surface_state, rebuild_surface_state},
    storage::Store,
};

pub fn inspect_payload(store: &Store, project_id: &str, tool: &str, node_id: &str) -> Result<InspectPayload> {
    let surface = load_surface_state(store, project_id, tool)?;
    inspect_payload_from_surface(store, project_id, &surface, node_id)
}

pub fn inspect_payload_from_surface(
    store: &Store,
    project_id: &str,
    surface: &crate::domain::SurfaceState,
    node_id: &str,
) -> Result<InspectPayload> {
    let entity = surface
        .nodes
        .iter()
        .find(|node| node.id() == node_id)
        .cloned()
        .ok_or_else(|| anyhow!("node not found"))?;
    let verdict = surface
        .verdicts
        .iter()
        .find(|verdict| verdict.entity_id == node_id)
        .cloned();
    let incoming_edges = surface
        .edges
        .iter()
        .filter(|edge| edge.to == node_id)
        .cloned()
        .collect();
    let outgoing_edges = surface
        .edges
        .iter()
        .filter(|edge| edge.from == node_id)
        .cloned()
        .collect();
    let related_activity = store
        .maybe_read_json(&store.activity_path(&surface.project.id, &surface.tool.id))?
        .unwrap_or_default();

    let viewer_content = entity_file_path(&entity).and_then(|path| fs::read_to_string(path).ok());
    let edit = editable_metadata(store, project_id, &entity, viewer_content.as_deref())?;

    Ok(InspectPayload {
        entity,
        verdict,
        incoming_edges,
        outgoing_edges,
        related_activity,
        viewer_content,
        edit,
    })
}

pub fn save_edit(
    config: &AppConfig,
    store: &Store,
    project_id: &str,
    tool: &str,
    node_id: &str,
    content: &str,
    version_token: &str,
) -> Result<SaveInspectResponse> {
    let surface = load_surface_state(store, project_id, tool)?;
    let entity = surface
        .nodes
        .iter()
        .find(|node| node.id() == node_id)
        .cloned()
        .context("node not found")?;
    let path = editable_path(&entity, Some(content))?;
    let current_content = fs::read_to_string(&path).context("read editable file")?;
    let current_token = file_version_token(&path)?;
    if current_token != version_token {
        return Err(anyhow!("File changed on disk. Reload before saving."));
    }

    let backup = EditBackup {
        path: path.to_string_lossy().to_string(),
        content: current_content,
        version_token: current_token,
    };
    store.write_json(&store.edit_backup_path(project_id, node_id), &backup)?;
    store.write_text_atomic(&path, content)?;

    let graph = rebuild_surface_state(config, store, project_id, tool)?;
    let inspect = inspect_payload_from_surface(store, project_id, &graph, node_id)?;
    Ok(SaveInspectResponse {
        inspect,
        graph,
        status_message: format!("Saved {}.", path.display()),
    })
}

pub fn revert_last_save(
    config: &AppConfig,
    store: &Store,
    project_id: &str,
    tool: &str,
    node_id: &str,
) -> Result<SaveInspectResponse> {
    let backup_path = store.edit_backup_path(project_id, node_id);
    let backup = store
        .maybe_read_json::<EditBackup>(&backup_path)?
        .context("No backup available for this file.")?;
    store.write_text_atomic(Path::new(&backup.path), &backup.content)?;
    let graph = rebuild_surface_state(config, store, project_id, tool)?;
    let inspect = inspect_payload_from_surface(store, project_id, &graph, node_id)?;
    Ok(SaveInspectResponse {
        inspect,
        graph,
        status_message: format!("Reverted {}.", backup.path),
    })
}

fn editable_metadata(
    store: &Store,
    project_id: &str,
    entity: &GraphNode,
    viewer_content: Option<&str>,
) -> Result<EditableMetadata> {
    let edit_path = entity_file_path(entity).map(|path| path.to_string_lossy().to_string());
    let editable = if let Some(path) = entity_file_path(entity) {
        viewer_content.is_some() && path.is_file()
    } else {
        false
    };
    let version_token = if editable {
        entity_file_path(entity)
            .map(file_version_token)
            .transpose()?
    } else {
        None
    };
    Ok(EditableMetadata {
        editable,
        edit_path,
        version_token,
        last_saved_backup_available: store
            .edit_backup_path(project_id, entity.id())
            .exists(),
    })
}

fn editable_path(entity: &GraphNode, proposed_content: Option<&str>) -> Result<std::path::PathBuf> {
    let path = entity_file_path(entity).context("Entity is not editable.")?;
    if proposed_content.is_none() && !path.is_file() {
        return Err(anyhow!("Editable target is not a regular file."));
    }
    Ok(path.to_path_buf())
}

fn entity_file_path(entity: &GraphNode) -> Option<&Path> {
    match entity {
        GraphNode::Artifact(node) => Some(Path::new(&node.path)),
        GraphNode::PluginArtifact(node) => Some(Path::new(&node.path)),
        _ => None,
    }
}

pub fn file_version_token(path: &Path) -> Result<String> {
    let metadata = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    hasher.update(&bytes);
    Ok(format!("{:x}:{mtime}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::Utc;
    use tempfile::TempDir;

    use crate::{
        catalogs::seed_catalog_map,
        config::AppConfig,
        domain::ProjectSummary,
        services::scan::{build_surface_state_for_test, collect_repo_files_for_test, display_path_for_test},
        storage::Store,
    };

    use super::{file_version_token, revert_last_save, save_edit};

    #[test]
    fn save_edit_blocks_on_conflict() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::write(repo.join("AGENTS.md"), "initial").expect("agents");

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
        store.ensure_layout().expect("layout");
        let inventory = collect_repo_files_for_test(&repo, 5);
        let project = ProjectSummary {
            id: "demo".to_string(),
            root_path: repo.to_string_lossy().to_string(),
            display_path: display_path_for_test(&repo, &home),
            name: "demo".to_string(),
            indexed_at: Utc::now(),
            status: "ready".to_string(),
        };
        store
            .write_json(&store.projects_index_path(), &vec![project.clone()])
            .expect("project index");
        store
            .write_json(&store.inventory_path(&project.id), &inventory)
            .expect("inventory");
        let state = build_surface_state_for_test(
            &config,
            &store,
            &project,
            &seed_catalog_map().expect("catalogs")["codex"],
            &inventory,
        )
        .expect("state");
        store
            .write_json(&store.tool_state_path(&project.id, "codex"), &state)
            .expect("tool state");
        let node_id = state
            .nodes
            .iter()
            .find(|node| node.id().starts_with("codex:"))
            .map(|node| node.id().to_string())
            .expect("node");

        let token = file_version_token(&repo.join("AGENTS.md")).expect("token");
        fs::write(repo.join("AGENTS.md"), "external").expect("external write");

        let error = save_edit(&config, &store, &project.id, "codex", &node_id, "draft", &token)
            .expect_err("conflict");
        assert!(error.to_string().contains("Reload before saving"));
    }

    #[test]
    fn revert_last_save_restores_previous_content() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        fs::create_dir_all(repo.join(".git")).expect("git dir");
        fs::write(repo.join("AGENTS.md"), "initial").expect("agents");

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
        store.ensure_layout().expect("layout");
        let inventory = collect_repo_files_for_test(&repo, 5);
        let project = ProjectSummary {
            id: "demo".to_string(),
            root_path: repo.to_string_lossy().to_string(),
            display_path: display_path_for_test(&repo, &home),
            name: "demo".to_string(),
            indexed_at: Utc::now(),
            status: "ready".to_string(),
        };
        store
            .write_json(&store.projects_index_path(), &vec![project.clone()])
            .expect("project index");
        store
            .write_json(&store.inventory_path(&project.id), &inventory)
            .expect("inventory");
        let state = build_surface_state_for_test(
            &config,
            &store,
            &project,
            &seed_catalog_map().expect("catalogs")["codex"],
            &inventory,
        )
        .expect("state");
        store
            .write_json(&store.tool_state_path(&project.id, "codex"), &state)
            .expect("tool state");
        let node_id = state
            .nodes
            .iter()
            .find(|node| node.id().starts_with("codex:"))
            .map(|node| node.id().to_string())
            .expect("node");

        let token = file_version_token(&repo.join("AGENTS.md")).expect("token");
        save_edit(&config, &store, &project.id, "codex", &node_id, "changed", &token)
            .expect("save");
        let response = revert_last_save(&config, &store, &project.id, "codex", &node_id)
            .expect("revert");

        assert_eq!(fs::read_to_string(repo.join("AGENTS.md")).expect("read"), "initial");
        assert_eq!(response.inspect.viewer_content.as_deref(), Some("initial"));
    }
}
