use std::net::SocketAddr;

use harness_inspector::{api, config::AppConfig, storage::Store};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = AppConfig::default()?;
    let store = Store::new(config.store_root.clone());
    store.ensure_layout()?;

    let app_state = api::AppState::new(config, store);
    let app = api::router(app_state);
    let addr = SocketAddr::from(([127, 0, 0, 1], 8765));

    tracing::info!("harness-inspector listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
