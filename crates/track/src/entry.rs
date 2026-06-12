use core::pin::pin;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow};
use clap::Parser;
use musli_web::ws::Channels;
use tokio::sync::{Notify, broadcast};
use tracing::Level;

use crate::app_broadcast::Broadcaster;
use crate::background;
use crate::cache::ImageCache;
use crate::db::{Database, OpenMode};
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;
use crate::web::{self, AppState};

#[derive(Parser)]
#[command(version, about = "Track web server")]
struct Args {
    /// Number of concurrent read connections to the database.
    #[arg(long, default_value_t = 16)]
    read_concurrency: usize,

    /// Path to the SQLite database file.
    #[arg(long, default_value = "track.db")]
    db: PathBuf,

    /// Directory for the image proxy disk cache.
    #[arg(long, default_value = "image-cache")]
    cache_dir: PathBuf,

    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:3000")]
    bind: SocketAddr,

    /// Add logging directives.
    #[arg(long)]
    log: Vec<String>,
}

pub async fn server() -> Result<ExitCode> {
    let args = Args::parse();

    let mut filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();

    for directive in &args.log {
        filter = filter.add_directive(directive.parse()?);
    }

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let db = Database::open(&args.db, OpenMode::Normal, args.read_concurrency)
        .with_context(|| anyhow!("Opening database at {}", args.db.display()))?;

    let http = reqwest::Client::builder()
        .user_agent("ontv-musli-web/0.1")
        .build()
        .context("Building HTTP client")?;

    let cache = ImageCache::new(&args.cache_dir);

    let (broadcast_tx, _) = broadcast::channel(64);
    let broadcast = Broadcaster::new(broadcast_tx);

    let queue = TaskQueue::new();
    let shutdown = Shutdown::new();

    let remote = RemoteClients::new(http.clone());

    let config = db.load_config().await.context("Loading config")?;
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
        cache,
        queue,
        remote,
        pending,
        config_changed,
    };

    tracing::info!("Listening on {}", args.bind);

    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .with_context(|| anyhow!("Binding to {}", args.bind))?;

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
                if let Err(error) = result.context("Server error") {
                    tracing::error!("Server error: {error:#}");
                    ok = false;
                } else {
                    tracing::info!("Server stopped");
                }

                stopped_server = true;
            }
            result = &mut background, if !stopped_background => {
                if let Err(error) = result.context("Background task panicked")
                    .and_then(|r| r.context("Background task error")) {
                    tracing::error!("Background task error: {error:#}");
                    ok = false;
                } else {
                    tracing::info!("Background task stopped");
                }

                stopped_background = true;
            }
            result = &mut queue_worker, if !stopped_queue => {
                if let Err(error) = result.context("Task queue panicked"){
                    tracing::error!("Task queue error: {error:#}");
                    ok = false;
                } else {
                    tracing::info!("Task queue stopped");
                }

                stopped_queue = true;
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received Ctrl-C, shutting down");
            }
        }

        shutdown.cancel();
    }

    if !ok {
        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}
