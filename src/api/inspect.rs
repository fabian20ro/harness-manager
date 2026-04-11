use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    api::{ApiError, ApiResult, AppState},
    domain::{InspectPayload, SaveInspectResponse},
    services::edit::{inspect_payload, revert_last_save, save_edit},
    services::validation::apply_fix,
    services::scan::reindex_project_tool_with_progress,
};

#[derive(Debug, Deserialize)]
pub struct InspectQuery {
    pub tool: String,
    pub node: String,
}

pub async fn get_inspect(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Query(query): Query<InspectQuery>,
) -> ApiResult<Json<InspectPayload>> {
    Ok(Json(
        inspect_payload(&state.store, &project_id, &query.tool, &query.node)
            .map_err(ApiError::from_inspect_error)?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct InspectSaveBody {
    pub tool: String,
    pub node: String,
    pub content: String,
    pub version_token: String,
}

pub async fn post_inspect_save(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(body): Json<InspectSaveBody>,
) -> ApiResult<Json<SaveInspectResponse>> {
    Ok(Json(
        save_edit(
            &state.config,
            &state.store,
            &project_id,
            &body.tool,
            &body.node,
            &body.content,
            &body.version_token,
        )
        .map_err(ApiError::from_edit_error)?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct InspectRevertBody {
    pub tool: String,
    pub node: String,
}

pub async fn post_inspect_revert_last_save(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(body): Json<InspectRevertBody>,
) -> ApiResult<Json<SaveInspectResponse>> {
    Ok(Json(
        revert_last_save(
            &state.config,
            &state.store,
            &project_id,
            &body.tool,
            &body.node,
        )
        .map_err(ApiError::from_edit_error)?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct InspectFixBody {
    pub tool: String,
    pub node: String,
    pub check_label: String,
}

pub async fn post_inspect_fix(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(body): Json<InspectFixBody>,
) -> ApiResult<Json<SaveInspectResponse>> {
    let payload = inspect_payload(&state.store, &project_id, &body.tool, &body.node)
        .map_err(ApiError::from_inspect_error)?;
    
    let artifact = match payload.entity {
        crate::domain::GraphNode::Artifact(ref a) => a,
        _ => return Err(ApiError::bad_request("Only artifacts can be fixed.")),
    };

    apply_fix(artifact, &body.check_label)?;

    // Reindex to update graph state
    let graph = reindex_project_tool_with_progress(
        &state.config,
        &state.store,
        &state.jobs,
        &project_id,
        &body.tool,
        |_| Ok(()),
    )?;

    // Refresh payload
    let updated_payload = inspect_payload(&state.store, &project_id, &body.tool, &body.node)
        .map_err(ApiError::from_inspect_error)?;

    Ok(Json(SaveInspectResponse {
        inspect: updated_payload,
        graph,
        status_message: format!("Fixed: {}", body.check_label),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;
    use crate::{
        config::AppConfig,
        domain::{ProjectKind, ProjectSummary, SurfaceState, ToolContext, ToolContextNode},
        storage::Store,
    };
    use axum::http::StatusCode;

    #[tokio::test]
    async fn get_inspect_returns_not_found_for_missing_node() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
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

        let project = ProjectSummary {
            id: "demo".to_string(),
            root_path: "/tmp/demo".to_string(),
            display_path: "~/git/demo".to_string(),
            name: "demo".to_string(),
            kind: ProjectKind::GitRepo,
            discovery_reason: String::new(),
            signal_score: 300,
            indexed_at: Utc::now(),
            status: "ready".to_string(),
        };
        let surface = SurfaceState {
            project: project.clone(),
            tool: ToolContext {
                id: "codex".to_string(),
                family: "codex".to_string(),
                display_name: "Codex".to_string(),
                catalog_version: "test".to_string(),
                support_level: "full".to_string(),
            },
            nodes: vec![crate::domain::GraphNode::ToolContext(ToolContextNode {
                id: "tool:codex".to_string(),
                tool: ToolContext {
                    id: "codex".to_string(),
                    family: "codex".to_string(),
                    display_name: "Codex".to_string(),
                    catalog_version: "test".to_string(),
                    support_level: "full".to_string(),
                },
            })],
            edges: Vec::new(),
            verdicts: Vec::new(),
            last_indexed_at: Utc::now(),
        };
        store
            .write_json(&store.tool_state_path(&project.id, "codex"), &surface)
            .expect("tool state");

        let state = AppState::new(config, store);
        let error = get_inspect(
            State(state),
            Path(project.id.clone()),
            Query(InspectQuery {
                tool: "codex".to_string(),
                node: "missing".to_string(),
            }),
        )
        .await
        .expect_err("missing node should fail");

        assert_eq!(error.status, StatusCode::NOT_FOUND);
        assert_eq!(error.message, "node not found");
    }
}
