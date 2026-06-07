mod app_broadcast;
mod background;
mod cache;
mod pending;
mod proxy;
mod remote;
#[cfg(feature = "bundle")]
mod static_assets;
mod sync;
mod task_queue;
mod tmdb;
mod tvdb;
mod tvmaze;
mod web;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;

use std::sync::Arc;

use anyhow::{Context as _, Result};
use clap::Parser;
use db::Database;
use musli_web::ws::Channels;
use tokio::sync::{Notify, broadcast};

use crate::app_broadcast::Broadcaster;
use crate::cache::ImageCache;
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::task_queue::TaskQueue;
use crate::web::AppState;

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

    let (broadcast_tx, _) = broadcast::channel(64);
    let broadcast = Broadcaster::new(broadcast_tx);

    let queue = TaskQueue::new();

    let remote = RemoteClients::new(http.clone());
    if let Ok(config) = db.load_config().await {
        remote.configure(&config);
    }

    let pending = PendingSystem::new(db.clone());
    let config_changed = Arc::new(Notify::new());

    let queue_worker = tokio::spawn(queue.clone().run(
        db.clone(),
        remote.clone(),
        broadcast.clone(),
        pending.clone(),
    ));

    let bg = tokio::spawn(background::run(
        db.clone(),
        queue.clone(),
        broadcast.clone(),
        remote.clone(),
        config_changed.clone(),
    ));

    let state = AppState {
        db,
        broadcast,
        channels: Channels::default(),
        http,
        cache,
        queue,
        remote,
        pending,
        config_changed,
    };

    tracing::info!("listening on {}", args.bind);

    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .context("failed to bind")?;

    let server = axum::serve(listener, web::router(state));

    tokio::select! {
        result = server => {
            result.context("server error")?;
        }
        result = bg => {
            result.context("background task panicked")?
                .context("background task error")?;
        }
        result = queue_worker => {
            result.context("task queue panicked")?;
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, shutting down");
        }
    }

    Ok(())
}
