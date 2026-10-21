use std::path::Path;
use std::sync::Arc;

use auth::Auth;
use axum::Router;
use axum::extract::FromRef;
use axum::routing::get;
use tokio::sync::Notify;
use tower_http::services::{ServeDir, ServeFile};

use crate::app_broadcast::Broadcaster;
use crate::cache::ImageCache;
use crate::db::Database;
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;
use crate::ws::RandomDelay;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: Database,
    pub(crate) broadcast: Broadcaster,
    pub(crate) cache: ImageCache,
    pub(crate) queue: TaskQueue,
    pub(crate) remote: RemoteClients,
    pub(crate) pending: PendingSystem,
    pub(crate) config_changed: Arc<Notify>,
    /// Optional artificial per-request websocket latency (dev/testing).
    pub(crate) delay: Option<RandomDelay>,
    pub(crate) shutdown: Shutdown,
    pub(crate) auth: Auth,
}

impl FromRef<AppState> for Auth {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}

pub(crate) fn router(state: AppState, dist: Option<&Path>) -> Router {
    let app = Router::new()
        .route("/ws", get(crate::ws::ws_handler))
        .route(
            "/api/image/{source}/{*path}",
            get(crate::proxy::image_handler),
        )
        .merge(auth::axum08::routes());

    // Unknown paths get index.html so client-side routes load.
    let app = match dist {
        Some(dir) => app
            .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))),
        #[cfg(feature = "bundle")]
        None => app.fallback(get(crate::static_assets::handler)),
        #[cfg(not(feature = "bundle"))]
        None => app,
    };

    app.with_state(state)
}
