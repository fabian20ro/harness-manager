use axum::{
    extract::State,
    response::Html,
    Json,
};
use serde::Deserialize;

use crate::{
    api::{ApiError, ApiResult, AppState},
    domain::{JobStatus, ToolCatalog},
    services::{
        activity::refresh_activity,
        docs::fetch_snapshot,
        scan::{load_catalogs, load_surface_state, refresh_catalogs},
    },
};

pub async fn index() -> Html<String> {
    if let Ok(content) = tokio::fs::read_to_string("ui/dist/index.html").await {
        return Html(content);
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
      ul { padding-left: 20px; }
      li { margin-bottom: 8px; }
      a { color: #0056b3; text-decoration: none; }
      a:hover { text-decoration: underline; }
    </style>
  </head>
  <body>
    <div class="card">
      <h1>Harness Inspector</h1>
      <p>Rust helper live. API endpoints:</p>
      <ul>
        <li><a href="/api/projects">GET /api/projects</a></li>
        <li><a href="/api/docs/fetch">POST /api/docs/fetch</a></li>
        <li><a href="/api/activity/refresh">POST /api/activity/refresh</a></li>
        <li><a href="/api/catalogs/refresh">POST /api/catalogs/refresh</a></li>
      </ul>
      <hr style="border: 0; border-top: 1px solid rgba(19,32,51,0.1); margin: 20px 0;">
      <p>React UI source in <code>ui/</code>. Run <code>npm install && npm run dev</code> there for the browser app.</p>
    </div>
  </body>
</html>"#
        .to_string()
    )
}

#[derive(Debug, Deserialize)]
pub struct DocFetchBody {
    pub url: String,
    pub project_id: Option<String>,
    pub tool: Option<String>,
}

pub async fn post_docs_fetch(
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
pub struct ActivityBody {
    pub project_id: String,
    pub tool: String,
}

pub async fn post_activity_refresh(
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
pub struct CatalogRefreshBody {
    pub catalogs: Option<Vec<ToolCatalog>>,
}

pub async fn post_catalog_refresh(
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    // Use a static mutex to prevent tests that change the process cwd from running in parallel.
    static CWD_MUTEX: Mutex<()> = Mutex::new(());

    struct TestCwd {
        original: std::path::PathBuf,
        _tempdir: tempfile::TempDir,
    }

    impl TestCwd {
        fn enter() -> Self {
            let original = std::env::current_dir().unwrap();
            let tempdir = tempfile::tempdir().unwrap();
            std::env::set_current_dir(tempdir.path()).unwrap();
            Self {
                original,
                _tempdir: tempdir,
            }
        }
    }

    impl Drop for TestCwd {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    #[tokio::test]
    async fn test_index_fallback() {
        let _lock = CWD_MUTEX.lock().unwrap();
        let _cwd = TestCwd::enter();

        let res = index().await;
        assert!(res.0.contains("Harness Inspector"));
        assert!(res.0.contains("Rust helper live"));
    }

    #[tokio::test]
    async fn test_index_file() {
        let _lock = CWD_MUTEX.lock().unwrap();
        let _cwd = TestCwd::enter();
        fs::create_dir_all("ui/dist").unwrap();
        fs::write("ui/dist/index.html", "<html><body>Custom Index</body></html>").unwrap();

        let res = index().await;
        assert_eq!(res.0, "<html><body>Custom Index</body></html>");
    }
}