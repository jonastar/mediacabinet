use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod api;
mod artwork;
mod transcode;
mod web;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub scans: cabinet_core::scan::ScanService,
    pub tasks: cabinet_core::tasks::Tasks,
    pub provider: cabinet_core::metadata::Provider,
    /// Rotten Tomatoes, Metacritic and IMDb ratings, when `CABINET_OMDB_KEY` is set.
    pub omdb: Option<cabinet_core::metadata::omdb::Omdb>,
    pub match_settings: cabinet_core::matcher::MatchSettings,
    /// Signing in with other accounts: sign-ins in progress, env providers.
    pub sign_in: cabinet_core::sign_in::SignIn,
    /// Devices pairing through a browser.
    pub pairings: cabinet_core::pairing::Pairings,
    pub artwork: artwork::ArtworkCache,
    pub transcoder: transcode::Transcoder,
    /// Where extracted frames are kept.
    pub thumbnailer: cabinet_core::thumbnails::Thumbnailer,
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
        Ok(key) if !key.trim().is_empty() => {
            let mut tmdb = Tmdb::new(key.trim());
            if let Ok(region) = std::env::var("CABINET_TMDB_REGION")
                && !region.trim().is_empty()
            {
                tmdb = tmdb.with_region(region.trim());
            }
            Ok(Provider::Tmdb(tmdb))
        }
        _ => {
            tracing::warn!("no metadata provider: set CABINET_TMDB_KEY to identify new folders");
            Ok(Provider::None)
        }
    }
}

/// OMDb, for Rotten Tomatoes and Metacritic ratings, with CABINET_OMDB_KEY.
fn omdb() -> Option<cabinet_core::metadata::omdb::Omdb> {
    let key = std::env::var("CABINET_OMDB_KEY").ok()?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some(cabinet_core::metadata::omdb::Omdb::new(key))
}

/// Providers from CABINET_OIDC_*, written to the database; problems are
/// logged and the server starts anyway.
async fn sign_in_providers(db: &sqlx::SqlitePool) -> anyhow::Result<cabinet_core::sign_in::SignIn> {
    use cabinet_core::sign_in::{SignIn, env, providers};
    let vars: Vec<(String, String)> = std::env::vars().collect();
    let password = env::password_sign_in(&vars).map_err(anyhow::Error::msg)?;
    let guests = env::guest_access(&vars).map_err(anyhow::Error::msg)?;
    let (from_env, problems) = env::parse(&vars);
    for problem in problems {
        tracing::warn!("{problem}");
    }
    for warning in providers::sync_env(db, &from_env).await? {
        tracing::warn!("{warning}");
    }
    if !from_env.is_empty() {
        let keys: Vec<&str> = from_env.iter().map(|p| p.key.as_str()).collect();
        tracing::info!(?keys, "sign-in providers from the environment");
    }
    // Guests outlive a restart only while guest access stays on.
    if cabinet_core::guests::enabled(db, guests).await? {
        cabinet_core::guests::sweep(db).await?;
    } else {
        cabinet_core::guests::remove_all(db).await?;
    }
    Ok(SignIn::new(from_env, password, guests))
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

    let (task_log, task_log_lines) = cabinet_core::tasks::log::layer();
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .with(task_log)
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
    let sign_in = sign_in_providers(&db).await?;

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
    let hwaccel = std::env::var("CABINET_HWACCEL").unwrap_or_default();
    let hwaccel = cabinet_core::playback::HwAccel::parse(&hwaccel)
        .context("CABINET_HWACCEL must be none, vaapi, vaapi:<device>, nvenc or qsv")?;
    let ffmpeg: PathBuf = std::env::var("CABINET_FFMPEG")
        .unwrap_or_else(|_| "ffmpeg".into())
        .into();
    let transcoder =
        transcode::Transcoder::new(data_dir.join("transcode"), ffmpeg.clone(), hwaccel).await;
    transcoder.spawn_reaper();

    let thumbnailer = cabinet_core::thumbnails::Thumbnailer {
        ffmpeg,
        dir: data_dir.join("thumbnails"),
    };
    let artwork = cabinet_core::artwork::ArtworkStore::new(data_dir.join("artwork"));
    cabinet_core::tasks::log::spawn_writer(db.clone(), task_log_lines);
    let omdb = omdb();
    let tasks = cabinet_core::tasks::Tasks::new(
        db.clone(),
        cabinet_core::tasks::Services {
            prober,
            provider: provider.clone(),
            match_settings,
            omdb: omdb.clone(),
            thumbnailer: Some(thumbnailer.clone()),
            artwork: Some(artwork.clone()),
            transcoding: {
                let transcoder = transcoder.clone();
                std::sync::Arc::new(move || transcoder.active())
            },
        },
    );
    tasks.resume().await?;
    cabinet_core::artwork::spawn_sweeper(
        tasks.clone(),
        artwork.clone(),
        Duration::from_secs(60 * 60),
    );
    let scans = cabinet_core::scan::ScanService::new(tasks.clone());
    let interval = env_number("CABINET_SCAN_INTERVAL_MINUTES", 360)?;
    if interval > 0 {
        scans.spawn_scheduler(Duration::from_secs(interval * 60));
    }
    if env_number("CABINET_WATCH", 1)? == 1 {
        scans.spawn_watcher(Duration::from_secs(10));
    }

    // Episodes that air become wanted without anything else changing.
    cabinet_core::wanted::spawn_syncer(db.clone(), Duration::from_secs(60));
    cabinet_core::activity::spawn_worker(db.clone());
    cabinet_core::webhooks::Deliverer::new(db.clone()).spawn(Duration::from_secs(2));
    cabinet_core::metadata::refresh::spawn_refresher(
        db.clone(),
        provider.clone(),
        Duration::from_secs(10 * 60),
    );
    if let Some(omdb) = &omdb {
        cabinet_core::metadata::omdb::spawn_refresher(
            db.clone(),
            omdb.clone(),
            Duration::from_secs(10 * 60),
        );
    }

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
        tasks,
        provider,
        omdb,
        match_settings,
        sign_in,
        pairings: cabinet_core::pairing::Pairings::default(),
        artwork: artwork::ArtworkCache(artwork),
        transcoder,
        thumbnailer,
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
