use core::pin::pin;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use clap::Parser;
use musli_web::ws::Channels;
use tokio::net::TcpListener;
use tokio::sync::{Notify, broadcast};
use tracing::Level;

use crate::app_broadcast::Broadcaster;
use crate::background;
use crate::cache::ImageCache;
use crate::db::{Database, OpenMode};
use crate::identity::Auth;
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;
use crate::web::{self, AppState};
use crate::ws::RandomDelay;

const READ_CONCURRENCY: usize = 16;
/// Bounds every remote request, so one hung connection cannot stall the
/// single-worker task queue.
const HTTP_TIMEOUT: Duration = Duration::from_secs(60);
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Parser)]
#[command(version, about = "Track web server")]
pub struct Args {
    /// Number of concurrent read connections to the database.
    #[arg(long, default_value_t = READ_CONCURRENCY)]
    read_concurrency: usize,

    /// Directory for the image proxy disk cache.
    #[arg(long, default_value = "image-cache")]
    cache_dir: PathBuf,

    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:3000")]
    bind: SocketAddr,

    /// Inject an artificial random delay into every websocket request, given as
    /// a `MIN..MAX` millisecond range, to preview loading/skeleton states on a
    /// slow connection (e.g. `--delay 200..800`).
    #[arg(long, value_name = "MIN..MAX")]
    delay: Option<RandomDelay>,

    /// Serve the frontend from this directory (a `trunk build` output) instead
    /// of the bundled one.
    #[arg(long, value_name = "DIR")]
    dist: Option<PathBuf>,
}

pub async fn server(args: Args, db: &Path, log: &[String]) -> Result<ExitCode> {
    let mut filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(Level::INFO.into())
        .from_env_lossy();

    for directive in log {
        filter = filter.add_directive(directive.parse()?);
    }

    tracing_subscriber::fmt().with_env_filter(filter).init();

    if let Some(delay) = args.delay {
        tracing::info!(?delay, "Injecting artificial websocket delay");
    }

    tracing::info!("Listening on {}", args.bind);

    let listener = TcpListener::bind(args.bind)
        .await
        .with_context(|| anyhow!("Binding to {}", args.bind))?;

    let ctrl_c = async {
        _ = tokio::signal::ctrl_c().await;
        tracing::info!("Received Ctrl-C, shutting down");
    };

    run(
        listener,
        db,
        args.read_concurrency,
        &args.cache_dir,
        args.delay,
        args.dist.as_deref(),
        ctrl_c,
    )
    .await
}

/// Run the server on an already bound `listener` against the database at `db`,
/// serving the frontend from `dist` (the bundled one when `None`), until
/// `shutdown` completes.
pub async fn serve(
    listener: TcpListener,
    db: &Path,
    cache_dir: &Path,
    dist: Option<&Path>,
    shutdown: impl Future<Output = ()>,
) -> Result<ExitCode> {
    run(
        listener,
        db,
        READ_CONCURRENCY,
        cache_dir,
        None,
        dist,
        shutdown,
    )
    .await
}

async fn run(
    listener: TcpListener,
    db: &Path,
    read_concurrency: usize,
    cache_dir: &Path,
    delay: Option<RandomDelay>,
    dist: Option<&Path>,
    shutdown_signal: impl Future<Output = ()>,
) -> Result<ExitCode> {
    let db = Database::open(db, OpenMode::Normal, read_concurrency)
        .with_context(|| anyhow!("Opening database at {}", db.display()))?;

    let http = reqwest::Client::builder()
        .user_agent("ontv/0.1.0")
        .connect_timeout(HTTP_CONNECT_TIMEOUT)
        .timeout(HTTP_TIMEOUT)
        .build()
        .context("Building HTTP client")?;

    let cache = ImageCache::new(cache_dir);

    let (broadcast_tx, _) = broadcast::channel(64);
    let broadcast = Broadcaster::new(broadcast_tx);

    let queue = TaskQueue::new();
    let shutdown = Shutdown::new();

    let remote = RemoteClients::new(http.clone());

    let config = db.load_config().await.context("Loading config")?;
    remote.configure(&config)?;

    let session_key = db.session_key().await.context("Loading session key")?;
    let auth = Auth::new(session_key, http.clone(), &config);

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
        delay,
        shutdown: shutdown.clone(),
        auth,
    };

    let server = {
        let shutdown = shutdown.clone();

        async move {
            let serve = axum::serve(
                listener,
                web::router(state, dist).into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(async move { shutdown.cancelled().await });

            serve.await?;
            Ok::<_, anyhow::Error>(())
        }
    };

    let mut server = pin!(server);
    let mut shutdown_signal = pin!(shutdown_signal);
    let mut signalled = false;

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
            _ = shutdown_signal.as_mut(), if !signalled => {
                signalled = true;
            }
        }

        shutdown.cancel();
    }

    if !ok {
        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}
