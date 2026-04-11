use std::{
    collections::HashMap,
    sync::Arc,
};

use axum::{
    http::{HeaderValue, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

use crate::{
    config::AppConfig,
    services::jobs::JobRegistry,
    services::scan::reindex_project_tool_with_progress,
    storage::Store,
};

pub mod projects;
pub mod inspect;
pub mod jobs;
pub mod meta;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub store: Store,
    pub jobs: JobRegistry,
}

impl AppState {
    pub fn new(config: AppConfig, store: Store) -> Self {
        let jobs = JobRegistry::new(store.clone());
        let state = Self {
            config: Arc::new(config),
            store,
            jobs,
        };
        
        // Start the background file watcher
        let watcher_state = state.clone();
        if let Err(e) = state.jobs.setup_watcher(move |path| {
            let s = watcher_state.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_file_event(&s, path).await {
                    tracing::error!("Failed to handle file event: {}", err);
                }
            });
        }) {
            tracing::error!("Failed to setup file watcher: {}", e);
        }

        state
    }
}

async fn handle_file_event(state: &AppState, path: std::path::PathBuf) -> anyhow::Result<()> {
    // Basic debounce/filter: only reindex if it's a file we care about
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if filename.starts_with('.') || filename == "target" || filename == "node_modules" {
        return Ok(());
    }

    // Find which project this path belongs to
    let projects = state.store.maybe_read_json::<Vec<crate::domain::ProjectSummary>>(
        &state.store.projects_index_path()
    )?.unwrap_or_default();

    for project in projects {
        if path.starts_with(&project.root_path) {
            tracing::info!("File change detected in project {}: {:?}", project.name, path);
            
            // Reindex the project for the current tool context
            // In a real multi-tool scenario, we'd iterate over all tools, 
            // but here we can start by re-indexing the main one or providing a global "Watch" job.
            let catalogs = crate::services::scan::load_catalogs(&state.store)?;
            for tool in catalogs.keys() {
                let _ = reindex_project_tool_with_progress(
                    &state.config,
                    &state.store,
                    &state.jobs,
                    &project.id,
                    tool,
                    |_| Ok(()),
                );
            }
        }
    }

    Ok(())
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
        .route("/", get(meta::index))
        .route("/api/projects", get(projects::get_projects))
        .route("/api/scan", post(projects::post_scan))
        .route("/api/projects/:id/reindex", post(projects::post_project_reindex))
        .route("/api/projects/:id/graph", get(projects::get_graph))
        .route("/api/projects/:id/inspect", get(inspect::get_inspect))
        .route("/api/projects/:id/inspect/save", post(inspect::post_inspect_save))
        .route(
            "/api/projects/:id/inspect/revert-last-save",
            post(inspect::post_inspect_revert_last_save),
        )
        .route("/api/projects/:id/inspect/fix", post(inspect::post_inspect_fix))
        .route("/api/docs/fetch", post(meta::post_docs_fetch))
        .route("/api/activity/refresh", post(meta::post_activity_refresh))
        .route("/api/catalogs/refresh", post(meta::post_catalog_refresh))
        .route("/api/jobs/:id", get(jobs::get_job))
        .route("/api/events", get(jobs::get_events))
        .nest_service("/assets", ServeDir::new("ui/dist/assets"))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.to_string(),
        }
    }

    pub fn internal(error: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }

    pub fn conflict(message: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.to_string(),
        }
    }

    pub fn bad_request(message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.to_string(),
        }
    }

    pub fn from_edit_error(error: anyhow::Error) -> Self {
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

    pub fn from_inspect_error(error: anyhow::Error) -> Self {
        let message = error.to_string();
        if message.contains("node not found") {
            Self::not_found(&message)
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
