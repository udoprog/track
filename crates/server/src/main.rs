mod app_broadcast;
mod background;
mod cache;
mod pending;
mod proxy;
mod remote;
mod shutdown;
#[cfg(feature = "bundle")]
mod static_assets;
mod sync;
mod task_queue;
mod tmdb;
mod tvdb;
mod tvmaze;
mod web;
mod ws;

use core::pin::pin;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
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
use crate::shutdown::Shutdown;
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
async fn main() -> Result<ExitCode> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
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
    let shutdown = Shutdown::new();

    let remote = RemoteClients::new(http.clone());

    let config = db.load_config().await.context("loading config")?;
    remote.configure(&config)?;

    let pending = PendingSystem::new(db.clone());
    let config_changed = Arc::new(Notify::new());

    let mut queue_worker = tokio::spawn(queue.clone().run(
        db.clone(),
        remote.clone(),
        broadcast.clone(),
        pending.clone(),
        shutdown.clone(),
    ));

    let mut background = tokio::spawn(background::run(
        db.clone(),
        queue.clone(),
        broadcast.clone(),
        config_changed.clone(),
        shutdown.clone(),
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

    let server = {
        let shutdown = shutdown.clone();

        async move {
            let serve = axum::serve(listener, web::router(state))
                .with_graceful_shutdown(async move { shutdown.cancelled().await });

            serve.await?;
            Ok::<_, anyhow::Error>(())
        }
    };

    let mut server = pin!(server);

    let mut stopped_server = false;
    let mut stopped_background = false;
    let mut stopped_queue = false;
    let mut ok = true;

    while !stopped_server || !stopped_background || !stopped_queue {
        tokio::select! {
            result = server.as_mut(), if !stopped_server => {
                if let Err(error) = result.context("server error") {
                    tracing::error!("server error: {error:#}");
                    ok = false;
                } else {
                    tracing::info!("server stopped");
                }

                stopped_server = true;
            }
            result = &mut background, if !stopped_background => {
                if let Err(error) = result.context("background task panicked")
                    .and_then(|r| r.context("background task error")) {
                    tracing::error!("background task error: {error:#}");
                    ok = false;
                } else {
                    tracing::info!("background task stopped");
                }

                stopped_background = true;
            }
            result = &mut queue_worker, if !stopped_queue => {
                if let Err(error) = result.context("task queue panicked"){
                    tracing::error!("task queue error: {error:#}");
                    ok = false;
                } else {
                    tracing::info!("task queue stopped");
                }

                stopped_queue = true;
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received ctrl-c, shutting down");
            }
        }

        shutdown.cancel();
    }

    if !ok {
        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}
