use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderValue, Method, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Html, IntoResponse,
    },
    routing::{get, post},
    Json, Router,
};
use futures_util::Stream;
use serde::Deserialize;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

use crate::{
    config::AppConfig,
    domain::{InspectPayload, JobStatus, SaveInspectResponse, SurfaceState, ToolCatalog},
    services::{
        activity::refresh_activity,
        docs::fetch_snapshot,
        edit::{inspect_payload, revert_last_save, save_edit},
        jobs::JobRegistry,
        scan::{load_catalogs, load_surface_state, refresh_catalogs, scan_projects},
    },
    storage::Store,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub store: Store,
    pub jobs: JobRegistry,
}

impl AppState {
    pub fn new(config: AppConfig, store: Store) -> Self {
        let jobs = JobRegistry::new(store.clone());
        Self {
            config: Arc::new(config),
            store,
            jobs,
        }
    }
}

pub fn router(state: AppState) -> Router {
    let allowed_origins = state
        .config
        .allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect::<Vec<_>>();
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([axum::http::header::CONTENT_TYPE])
        .allow_origin(allowed_origins);

    Router::new()
        .route("/", get(index))
        .route("/api/projects", get(get_projects))
        .route("/api/scan", post(post_scan))
        .route("/api/projects/:id/graph", get(get_graph))
        .route("/api/projects/:id/inspect", get(get_inspect))
        .route("/api/projects/:id/inspect/save", post(post_inspect_save))
        .route(
            "/api/projects/:id/inspect/revert-last-save",
            post(post_inspect_revert_last_save),
        )
        .route("/api/docs/fetch", post(post_docs_fetch))
        .route("/api/activity/refresh", post(post_activity_refresh))
        .route("/api/catalogs/refresh", post(post_catalog_refresh))
        .route("/api/jobs/:id", get(get_job))
        .route("/api/events", get(get_events))
        .nest_service("/assets", ServeDir::new("ui/dist/assets"))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    if let Ok(content) = std::fs::read_to_string("ui/dist/index.html") {
        let leaked = Box::leak(content.into_boxed_str());
        return Html(leaked);
    }

    Html(
        r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <title>Harness Inspector</title>
    <style>
      body { font-family: ui-sans-serif, system-ui, sans-serif; margin: 0; padding: 40px; background: linear-gradient(180deg,#f8f2e8,#edf5ff); color: #132033; }
      code { background: rgba(19,32,51,0.08); padding: 2px 6px; border-radius: 6px; }
      .card { max-width: 720px; background: rgba(255,255,255,0.76); border: 1px solid rgba(19,32,51,0.1); padding: 24px; border-radius: 18px; box-shadow: 0 24px 80px rgba(19,32,51,0.08); }
    </style>
  </head>
  <body>
    <div class="card">
      <h1>Harness Inspector</h1>
      <p>Rust helper live. API on <code>/api/*</code>.</p>
      <p>React UI source in <code>ui/</code>. Run <code>npm install && npm run dev</code> there for the browser app.</p>
    </div>
  </body>
</html>"#,
    )
}

async fn get_projects(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<crate::domain::ProjectSummary>>> {
    let projects = state
        .store
        .maybe_read_json(&state.store.projects_index_path())?
        .unwrap_or_default();
    Ok(Json(projects))
}

#[derive(Debug, Deserialize)]
struct ScanBody {
    roots: Option<Vec<String>>,
}

async fn post_scan(
    State(state): State<AppState>,
    Json(body): Json<Option<ScanBody>>,
) -> ApiResult<Json<JobStatus>> {
    let job = state.jobs.create("scan", "Scanning projects.")?;
    let result = scan_projects(
        &state.config,
        &state.store,
        body.and_then(|body| body.roots),
    );
    let job = match result {
        Ok(projects) => state.jobs.finish(
            job,
            "completed",
            &format!("Indexed {} project(s).", projects.len()),
        )?,
        Err(error) => state.jobs.finish(job, "failed", &error.to_string())?,
    };
    Ok(Json(job))
}

#[derive(Debug, Deserialize)]
struct ToolQuery {
    tool: String,
}

async fn get_graph(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Query(query): Query<ToolQuery>,
) -> ApiResult<Json<SurfaceState>> {
    Ok(Json(load_surface_state(
        &state.store,
        &project_id,
        &query.tool,
    )?))
}

#[derive(Debug, Deserialize)]
struct InspectQuery {
    tool: String,
    node: String,
}

async fn get_inspect(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Query(query): Query<InspectQuery>,
) -> ApiResult<Json<InspectPayload>> {
    Ok(Json(
        inspect_payload(&state.store, &project_id, &query.tool, &query.node)
            .map_err(ApiError::internal)?,
    ))
}

#[derive(Debug, Deserialize)]
struct InspectSaveBody {
    tool: String,
    node: String,
    content: String,
    version_token: String,
}

async fn post_inspect_save(
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
struct InspectRevertBody {
    tool: String,
    node: String,
}

async fn post_inspect_revert_last_save(
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
struct DocFetchBody {
    url: String,
    project_id: Option<String>,
    tool: Option<String>,
}

async fn post_docs_fetch(
    State(state): State<AppState>,
    Json(body): Json<DocFetchBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let job = state.jobs.create("fetch-docs", "Fetching docs snapshot.")?;
    let result = fetch_snapshot(
        &state.config,
        &state.store,
        &body.url,
        body.project_id.as_deref(),
        body.tool.as_deref(),
    )
    .await;
    let payload = match result {
        Ok((snapshot, association)) => {
            state
                .jobs
                .finish(job, "completed", "Docs snapshot fetched.")?;
            serde_json::json!({ "snapshot": snapshot, "association": association })
        }
        Err(error) => {
            state.jobs.finish(job, "failed", &error.to_string())?;
            return Err(ApiError::internal(error));
        }
    };
    Ok(Json(payload))
}

#[derive(Debug, Deserialize)]
struct ActivityBody {
    project_id: String,
    tool: String,
}

async fn post_activity_refresh(
    State(state): State<AppState>,
    Json(body): Json<ActivityBody>,
) -> ApiResult<Json<JobStatus>> {
    let job = state
        .jobs
        .create("refresh-activity", "Refreshing process-based observations.")?;
    let surface = load_surface_state(&state.store, &body.project_id, &body.tool)?;
    let catalogs = load_catalogs(&state.store)?;
    let catalog = catalogs
        .get(&body.tool)
        .ok_or_else(|| ApiError::not_found("catalog not found"))?;
    let result = refresh_activity(
        &state.config,
        &state.store,
        &body.project_id,
        std::path::Path::new(&surface.project.root_path),
        catalog,
    );
    let job = match result {
        Ok(evidence) => state.jobs.finish(
            job,
            "completed",
            &format!("Collected {} observation(s).", evidence.len()),
        )?,
        Err(error) => state.jobs.finish(job, "failed", &error.to_string())?,
    };
    Ok(Json(job))
}

#[derive(Debug, Deserialize)]
struct CatalogRefreshBody {
    catalogs: Option<Vec<ToolCatalog>>,
}

async fn post_catalog_refresh(
    State(state): State<AppState>,
    Json(body): Json<Option<CatalogRefreshBody>>,
) -> ApiResult<Json<JobStatus>> {
    let job = state
        .jobs
        .create("refresh-tool-catalog", "Refreshing tool catalogs.")?;
    let result = refresh_catalogs(&state.store, body.and_then(|body| body.catalogs));
    let job = match result {
        Ok(catalogs) => state.jobs.finish(
            job,
            "completed",
            &format!(
                "Catalog refresh complete for {} surface(s).",
                catalogs.len()
            ),
        )?,
        Err(error) => state.jobs.finish(job, "failed", &error.to_string())?,
    };
    Ok(Json(job))
}

async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> ApiResult<Json<JobStatus>> {
    let job = state
        .jobs
        .get(&job_id)?
        .ok_or_else(|| ApiError::not_found("job not found"))?;
    Ok(Json(job))
}

async fn get_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let stream = BroadcastStream::new(state.jobs.subscribe()).filter_map(|result| {
        result
            .ok()
            .map(|job| Ok(Event::default().json_data(job).expect("job to json")))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

type ApiResult<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.to_string(),
        }
    }

    fn internal(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }

    fn conflict(message: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.to_string(),
        }
    }

    fn bad_request(message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.to_string(),
        }
    }

    fn from_edit_error(error: anyhow::Error) -> Self {
        let message = error.to_string();
        if message.contains("Reload before saving") {
            Self::conflict(&message)
        } else if message.contains("No backup available")
            || message.contains("Entity is not editable")
            || message.contains("node not found")
        {
            Self::bad_request(&message)
        } else {
            Self::internal(error)
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::internal(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(HashMap::from([("error", self.message)]))).into_response()
    }
}
