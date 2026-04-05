use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::Stream;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};

use crate::{
    api::{ApiError, ApiResult, AppState},
    domain::JobStatus,
};

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
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let stream = BroadcastStream::new(state.jobs.subscribe()).filter_map(|result| {
        result
            .ok()
            .map(|job| Ok(Event::default().json_data(job).expect("job to json")))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
