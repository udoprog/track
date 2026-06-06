mod cache;
mod proxy;
mod remote;
#[cfg(feature = "bundle")]
mod static_assets;
mod sync;
mod task_queue;
mod tmdb;
mod tvdb;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context as _, Result};
use api::AppEvent;
use axum::Router;
use axum::routing::get;
use cache::ImageCache;
use clap::Parser;
use db::Database;
use musli_web::ws::Channels;
use remote::RemoteClients;
use task_queue::TaskQueue;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

#[derive(Clone)]
struct AppState {
    db: Database,
    broadcast: broadcast::Sender<AppEvent>,
    channels: Channels,
    http: reqwest::Client,
    cache: ImageCache,
    queue: TaskQueue,
    remote: RemoteClients,
}

#[derive(Parser)]
#[command(version, about = "OnTV web server")]
struct Args {
    /// Path to the SQLite database file.
    #[arg(long, default_value = "ontv.db")]
    db_path: PathBuf,

    /// Directory for the image proxy disk cache.
    #[arg(long, default_value = "image-cache")]
    cache_dir: PathBuf,

    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:3000")]
    bind: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().context("invalid tracing directive")?),
        )
        .init();

    let args = Args::parse();

    let db =
        Database::open(&args.db_path, db::OpenMode::Normal).context("failed to open database")?;

    let http = reqwest::Client::builder()
        .user_agent("ontv-musli-web/0.1")
        .build()
        .context("building HTTP client")?;

    let cache = ImageCache::new(&args.cache_dir);

    let (broadcast, _) = broadcast::channel(64);

    let queue = TaskQueue::new();

    let remote = RemoteClients::new(http.clone());
    if let Ok(config) = db.load_config().await {
        remote.configure(&config);
    }

    // Spawn the task queue worker.
    tokio::spawn(
        queue
            .clone()
            .run(db.clone(), remote.clone(), broadcast.clone()),
    );

    // Spawn the automatic background sync loop.
    tokio::spawn({
        let db = db.clone();
        let queue = queue.clone();
        let broadcast = broadcast.clone();
        async move {
            loop {
                let config = db.load_config().await.unwrap_or_default();
                let hours = config.auto_sync_interval_hours.max(1) as u64;
                tokio::time::sleep(std::time::Duration::from_secs(hours * 3600)).await;

                let config = db.load_config().await.unwrap_or_default();
                if !config.auto_sync_enabled {
                    continue;
                }

                for s in db.series().await.unwrap_or_default() {
                    queue
                        .push(
                            api::TaskKind::SyncSeries {
                                series_id: s.id,
                                title: s.title,
                            },
                            false,
                            &broadcast,
                        )
                        .await;
                }
                for m in db.movies().await.unwrap_or_default() {
                    queue
                        .push(
                            api::TaskKind::SyncMovie {
                                movie_id: m.id,
                                title: m.title,
                            },
                            false,
                            &broadcast,
                        )
                        .await;
                }
            }
        }
    });

    let state = AppState {
        db,
        broadcast,
        channels: Channels::default(),
        http,
        cache,
        queue,
        remote,
    };

    let app = Router::new();
    let app = app.route("/ws", get(ws::ws_handler));
    let app = app.route("/api/image/{source}/{*path}", get(proxy::image_handler));

    #[cfg(feature = "bundle")]
    let app = app.fallback(get(static_assets::handler));

    let app = app.layer(CorsLayer::permissive()).with_state(state);

    tracing::info!("listening on {}", args.bind);

    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .context("failed to bind")?;

    let server = axum::serve(listener, app);

    tokio::select! {
        result = server => {
            result.context("server error")?;
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, shutting down");
        }
    }

    Ok(())
}
