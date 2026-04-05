use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Result;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    api::{ApiError, ApiResult, AppState},
    config::AppConfig,
    domain::{JobStatus, SurfaceState},
    services::{
        jobs::{JobRegistry, JobUpdate},
        scan::{
            load_surface_state, reindex_project_tool_with_progress,
            scan_projects_with_progress, ScanProgress,
        },
    },
    storage::Store,
};

async fn get_projects_internal(
    state: &AppState,
) -> ApiResult<Json<Vec<crate::domain::ProjectSummary>>> {
    let projects = state
        .store
        .maybe_read_json(&state.store.projects_index_path())?
        .unwrap_or_default();
    Ok(Json(projects))
}

pub async fn get_projects(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<crate::domain::ProjectSummary>>> {
    get_projects_internal(&state).await
}

#[derive(Debug, Deserialize)]
pub struct ScanBody {
    roots: Option<Vec<String>>,
}

pub async fn post_scan(
    State(state): State<AppState>,
    Json(body): Json<Option<ScanBody>>,
) -> ApiResult<Json<JobStatus>> {
    ensure_no_running_scan_job(&state.jobs)?;
    let job = state
        .jobs
        .create_scoped("scan", "Scanning projects.", Some("global"), None, None)?;
    spawn_scan_job(
        state.jobs.clone(),
        state.config.clone(),
        state.store.clone(),
        job.clone(),
        body.and_then(|payload| payload.roots),
    );
    Ok(Json(job))
}

#[derive(Debug, Deserialize)]
pub struct ProjectReindexBody {
    pub tool: String,
}

pub async fn post_project_reindex(
    State(state): State<AppState>,
    Path(project_id): Path<String>,
    Json(body): Json<ProjectReindexBody>,
) -> ApiResult<Json<JobStatus>> {
    ensure_no_running_scan_job(&state.jobs)?;
    let job = state.jobs.create_scoped(
        "scan",
        &format!("Reindexing {}.", body.tool),
        Some("project_tool"),
        Some(&project_id),
        Some(&body.tool),
    )?;
    spawn_project_reindex_job(
        state.jobs.clone(),
        state.config.clone(),
        state.store.clone(),
        job.clone(),
        project_id,
        body.tool,
    );
    Ok(Json(job))
}

pub fn ensure_no_running_scan_job(jobs: &JobRegistry) -> ApiResult<()> {
    if jobs.find_running_kind("scan").is_some() {
        return Err(ApiError::conflict("Another scan or reindex job is already running."));
    }
    Ok(())
}

pub fn spawn_scan_job(
    jobs: JobRegistry,
    config: Arc<AppConfig>,
    store: Store,
    job: JobStatus,
    roots: Option<Vec<String>>,
) {
    tokio::spawn(async move {
        let job_id = job.id.clone();
        let jobs_for_work = jobs.clone();
        let config_for_work = config.clone();
        let store_for_work = store.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut emitter = ScanProgressEmitter::new(jobs_for_work.clone(), job.clone());
            scan_projects_with_progress(&config_for_work, &store_for_work, roots, |progress| {
                emitter.emit(progress)
            })
        })
        .await;

        match result {
            Ok(Ok(projects)) => {
                let _ = finish_job_from_latest(
                    &jobs,
                    &job_id,
                    "completed",
                    &format!("Indexed {} project(s).", projects.len()),
                );
            }
            Ok(Err(error)) => {
                let _ = finish_job_from_latest(&jobs, &job_id, "failed", &error.to_string());
            }
            Err(error) => {
                let _ = finish_job_from_latest(&jobs, &job_id, "failed", &error.to_string());
            }
        }
    });
}

pub fn spawn_project_reindex_job(
    jobs: JobRegistry,
    config: Arc<AppConfig>,
    store: Store,
    job: JobStatus,
    project_id: String,
    tool: String,
) {
    tokio::spawn(async move {
        let job_id = job.id.clone();
        let jobs_for_work = jobs.clone();
        let config_for_work = config.clone();
        let store_for_work = store.clone();
        let project_id_for_work = project_id.clone();
        let tool_for_work = tool.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut emitter = ScanProgressEmitter::new(jobs_for_work.clone(), job.clone());
            reindex_project_tool_with_progress(
                &config_for_work,
                &store_for_work,
                &project_id_for_work,
                &tool_for_work,
                |progress| emitter.emit(progress),
            )
        })
        .await;

        match result {
            Ok(Ok(surface)) => {
                let _ = finish_job_from_latest(
                    &jobs,
                    &job_id,
                    "completed",
                    &format!(
                        "Reindexed {} for {}.",
                        surface.tool.display_name, surface.project.display_path
                    ),
                );
            }
            Ok(Err(error)) => {
                let _ = finish_job_from_latest(&jobs, &job_id, "failed", &error.to_string());
            }
            Err(error) => {
                let _ = finish_job_from_latest(&jobs, &job_id, "failed", &error.to_string());
            }
        }
    });
}

pub fn finish_job_from_latest(
    jobs: &JobRegistry,
    job_id: &str,
    status: &str,
    message: &str,
) -> Result<JobStatus> {
    let job = jobs
        .get(job_id)?
        .ok_or_else(|| anyhow::anyhow!("job not found during finish"))?;
    jobs.finish(job, status, message)
}

pub struct ScanProgressEmitter {
    jobs: JobRegistry,
    job: JobStatus,
    last_emit_at: Option<Instant>,
    last_message: String,
    last_path: Option<String>,
}

impl ScanProgressEmitter {
    pub fn new(jobs: JobRegistry, job: JobStatus) -> Self {
        Self {
            jobs,
            last_message: job.message.clone(),
            last_path: job.current_path.clone(),
            job,
            last_emit_at: None,
        }
    }

    pub fn emit(&mut self, progress: ScanProgress) -> Result<()> {
        let path_changed = self.last_path != progress.current_path;
        let message_changed = self.last_message != progress.message;
        let throttled = self
            .last_emit_at
            .is_some_and(|instant| instant.elapsed() < Duration::from_millis(250));
        if !path_changed && !message_changed && throttled {
            return Ok(());
        }

        self.job = self.jobs.update(
            self.job.clone(),
            JobUpdate {
                message: Some(progress.message.clone()),
                phase: Some(Some(progress.phase)),
                current_path: Some(progress.current_path.clone()),
                items_done: Some(progress.items_done),
                items_total: Some(progress.items_total),
                ..JobUpdate::default()
            },
        )?;
        self.last_emit_at = Some(Instant::now());
        self.last_message = progress.message;
        self.last_path = progress.current_path;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct ToolQuery {
    pub tool: String,
}

pub async fn get_graph(
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;
    use tokio::time::sleep;
    use crate::domain::{ProjectKind, ProjectSummary};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn post_scan_returns_running_job_immediately_and_other_requests_still_work() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        std::fs::create_dir_all(repo.join(".git")).expect("git dir");
        std::fs::write(repo.join("AGENTS.md"), "policy\n").expect("agents");

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
        let state = AppState::new(config, store);

        let Json(job) = post_scan(State(state.clone()), Json(None))
            .await
            .expect("scan start");
        assert_eq!(job.status, "running");
        assert_eq!(job.kind, "scan");
        assert_eq!(job.scope_kind.as_deref(), Some("global"));

        let Json(_projects) = get_projects(State(state.clone()))
            .await
            .expect("projects request");

        sleep(Duration::from_millis(10)).await;
        let persisted = state.jobs.get(&job.id).expect("load job").expect("job present");
        assert_eq!(persisted.id, job.id);
    }

    #[tokio::test]
    async fn post_project_reindex_returns_running_job_immediately() {
        let temp = TempDir::new().expect("tempdir");
        let home = temp.path().join("home");
        let repo = home.join("git").join("demo");
        std::fs::create_dir_all(repo.join(".git")).expect("git dir");

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
            root_path: repo.to_string_lossy().to_string(),
            display_path: "~/git/demo".to_string(),
            name: "demo".to_string(),
            kind: ProjectKind::GitRepo,
            discovery_reason: String::new(),
            signal_score: 300,
            indexed_at: Utc::now(),
            status: "ready".to_string(),
        };
        store
            .write_json(&store.projects_index_path(), &vec![project])
            .expect("project index");

        let state = AppState::new(config, store);
        let Json(job) = post_project_reindex(
            State(state),
            Path("demo".to_string()),
            Json(ProjectReindexBody {
                tool: "codex".to_string(),
            }),
        )
        .await
        .expect("reindex start");

        assert_eq!(job.status, "running");
        assert_eq!(job.kind, "scan");
        assert_eq!(job.scope_kind.as_deref(), Some("project_tool"));
        assert_eq!(job.project_id.as_deref(), Some("demo"));
        assert_eq!(job.tool.as_deref(), Some("codex"));
    }

    #[tokio::test]
    async fn scan_start_rejects_when_another_scan_job_is_running() {
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
        let state = AppState::new(config, store);
        state
            .jobs
            .create_scoped("scan", "Scanning projects.", Some("global"), None, None)
            .expect("running job");

        let error = post_scan(State(state), Json(None))
            .await
            .expect_err("scan should conflict");
        assert_eq!(error.status, StatusCode::CONFLICT);
        assert_eq!(error.message, "Another scan or reindex job is already running.");
    }
}
