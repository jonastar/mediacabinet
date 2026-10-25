use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

mod api;
mod web;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        println!("{}", api::spec().to_pretty_json()?);
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let data_dir =
        PathBuf::from(std::env::var("CABINET_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let bind: SocketAddr = std::env::var("CABINET_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8420".into())
        .parse()
        .context("CABINET_BIND must be an address like 0.0.0.0:8420")?;

    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating {}", data_dir.display()))?;
    let db = cabinet_core::db::open(&data_dir.join("cabinet.db")).await?;

    if std::env::args().nth(1).as_deref() == Some("seed-demo") {
        let media = data_dir.join("demo-media");
        cabinet_core::demo::seed(&db, &media, cabinet_core::catalog::now()).await?;
        println!("Seeded demo data into {}", data_dir.display());
        return Ok(());
    }

    let app = api::router(AppState { db })
        .fallback(web::serve)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, data_dir = %data_dir.display(), "Media Cabinet listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
