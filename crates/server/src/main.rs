use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

mod api;
mod artwork;
mod transcode;
mod web;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub scans: cabinet_core::scan::ScanService,
    pub provider: cabinet_core::metadata::Provider,
    pub match_settings: cabinet_core::matcher::MatchSettings,
    pub artwork: artwork::ArtworkCache,
    pub transcoder: transcode::Transcoder,
    /// Derived files that can be rebuilt, such as subtitles as WebVTT.
    pub cache: PathBuf,
    /// Turns true when the server starts shutting down, so endless streams can end.
    pub shutdown: tokio::sync::watch::Receiver<bool>,
}

/// Resolves once `shutdown` turns true.
pub async fn shutting_down(mut shutdown: tokio::sync::watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|down| *down).await;
}

/// Ctrl-C, or SIGTERM (e.g. `docker stop`).
async fn stop_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("installing a SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// How long open requests (a video stream, a long poll) get to finish on shutdown.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// TMDB with CABINET_TMDB_KEY, or an offline catalog with CABINET_METADATA_CATALOG.
fn metadata_provider() -> anyhow::Result<cabinet_core::metadata::Provider> {
    use cabinet_core::metadata::{Provider, fake::Catalog, tmdb::Tmdb};
    if let Ok(path) = std::env::var("CABINET_METADATA_CATALOG") {
        let json = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
        let catalog = Catalog::from_json(&json).with_context(|| format!("parsing {path}"))?;
        tracing::warn!(
            path,
            "using an offline metadata catalog instead of a provider"
        );
        return Ok(Provider::Fake(std::sync::Arc::new(catalog)));
    }
    match std::env::var("CABINET_TMDB_KEY") {
        Ok(key) if !key.trim().is_empty() => Ok(Provider::Tmdb(Tmdb::new(key.trim()))),
        _ => {
            tracing::warn!("no metadata provider: set CABINET_TMDB_KEY to identify new folders");
            Ok(Provider::None)
        }
    }
}

fn env_number(name: &str, default: u64) -> anyhow::Result<u64> {
    match std::env::var(name) {
        Ok(v) => v
            .parse()
            .with_context(|| format!("{name} must be a whole number")),
        Err(_) => Ok(default),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("openapi") {
        println!("{}", api::spec().to_pretty_json()?);
        return Ok(());
    }

    // Local development settings; real environment variables win.
    let env_file = dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    if let Some(path) = env_file {
        tracing::info!(path = %path.display(), "loaded settings from .env");
    }

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

    // The first admin; the setup screen gives it a name and password.
    cabinet_core::catalog::ensure_default_user(&db).await?;

    let prober = cabinet_core::scan::Prober {
        program: std::env::var("CABINET_FFPROBE")
            .unwrap_or_else(|_| "ffprobe".into())
            .into(),
        ..Default::default()
    };
    let provider = metadata_provider()?;
    let match_settings = cabinet_core::matcher::MatchSettings {
        accept: match std::env::var("CABINET_MATCH_THRESHOLD") {
            Ok(v) => v
                .parse()
                .context("CABINET_MATCH_THRESHOLD must be a number between 0 and 1")?,
            Err(_) => cabinet_core::matcher::MatchSettings::default().accept,
        },
    };
    let scans = cabinet_core::scan::ScanService::with_matching(
        db.clone(),
        prober,
        provider.clone(),
        match_settings,
    );
    scans.recover().await?;
    let interval = env_number("CABINET_SCAN_INTERVAL_MINUTES", 360)?;
    if interval > 0 {
        scans.spawn_scheduler(Duration::from_secs(interval * 60));
    }
    if env_number("CABINET_WATCH", 1)? == 1 {
        scans.spawn_watcher(Duration::from_secs(10));
    }

    // Episodes that air become wanted without anything else changing.
    cabinet_core::wanted::spawn_syncer(db.clone(), Duration::from_secs(60));
    cabinet_core::webhooks::Deliverer::new(db.clone()).spawn(Duration::from_secs(2));
    cabinet_core::metadata::refresh::spawn_refresher(
        db.clone(),
        provider.clone(),
        Duration::from_secs(10 * 60),
    );

    let hwaccel = std::env::var("CABINET_HWACCEL").unwrap_or_default();
    let hwaccel = cabinet_core::playback::HwAccel::parse(&hwaccel)
        .context("CABINET_HWACCEL must be none, vaapi, vaapi:<device>, nvenc or qsv")?;
    let transcoder = transcode::Transcoder::new(
        data_dir.join("transcode"),
        std::env::var("CABINET_FFMPEG")
            .unwrap_or_else(|_| "ffmpeg".into())
            .into(),
        hwaccel,
    )
    .await;
    transcoder.spawn_reaper();

    let (stop, shutdown) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        stop_signal().await;
        tracing::info!("shutting down; press Ctrl-C again to quit now");
        let _ = stop.send(true);
        stop_signal().await;
        std::process::exit(130);
    });

    let app = api::router(AppState {
        db,
        scans,
        provider,
        match_settings,
        artwork: artwork::ArtworkCache::new(data_dir.join("artwork")),
        transcoder,
        cache: data_dir.join("cache"),
        shutdown: shutdown.clone(),
    })
    .fallback(web::serve)
    .layer(tower_http::trace::TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(%bind, data_dir = %data_dir.display(), "Media Cabinet listening");
    let server = axum::serve(listener, app).with_graceful_shutdown(shutting_down(shutdown.clone()));
    tokio::select! {
        served = server.into_future() => served?,
        _ = async {
            shutting_down(shutdown).await;
            tokio::time::sleep(SHUTDOWN_GRACE).await;
        } => tracing::warn!("requests still open after {SHUTDOWN_GRACE:?}; quitting anyway"),
    }
    Ok(())
}
