use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::Stream;
use serde::Deserialize;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::{
    api::{ApiError, ApiResult, AppState},
    domain::JobStatus,
};

#[derive(Debug, Deserialize)]
pub struct JobEventQuery {
    pub job_id: Option<String>,
    pub tool: Option<String>,
    pub kind: Option<String>,
    pub project_id: Option<String>,
}

pub async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> ApiResult<Json<JobStatus>> {
    let job = state
        .jobs
        .get(&job_id)?
        .ok_or_else(|| ApiError::not_found("job not found"))?;
    Ok(Json(job))
}

pub async fn get_events(
    State(state): State<AppState>,
    Query(query): Query<JobEventQuery>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let stream = BroadcastStream::new(state.jobs.subscribe()).filter_map(move |result| {
        let job = result.ok()?;
        if let Some(ref target_id) = query.job_id {
            if &job.id != target_id {
                return None;
            }
        }
        if let Some(ref target_tool) = query.tool {
            if job.tool.as_deref() != Some(target_tool) {
                return None;
            }
        }
        if let Some(ref target_kind) = query.kind {
            if job.kind != *target_kind {
                return None;
            }
        }
        if let Some(ref target_project_id) = query.project_id {
            if job.project_id.as_deref() != Some(target_project_id) {
                return None;
            }
        }
        Some(Ok(Event::default().json_data(job).expect("job to json")))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
