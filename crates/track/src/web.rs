use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use musli_web::ws::Channels;
use tokio::sync::Notify;
use tower_http::cors::CorsLayer;

use crate::app_broadcast::Broadcaster;
use crate::cache::ImageCache;
use crate::db::Database;
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::task_queue::TaskQueue;
use crate::ws::RandomDelay;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: Database,
    pub(crate) broadcast: Broadcaster,
    pub(crate) channels: Channels,
    pub(crate) cache: ImageCache,
    pub(crate) queue: TaskQueue,
    pub(crate) remote: RemoteClients,
    pub(crate) pending: PendingSystem,
    pub(crate) config_changed: Arc<Notify>,
    /// Optional artificial per-request websocket latency (dev/testing).
    pub(crate) delay: Option<RandomDelay>,
}

pub(crate) fn router(state: AppState) -> Router {
    let app = Router::new()
        .route("/ws", get(crate::ws::ws_handler))
        .route(
            "/api/image/{source}/{*path}",
            get(crate::proxy::image_handler),
        );

    #[cfg(feature = "bundle")]
    let app = app.fallback(get(crate::static_assets::handler));

    app.layer(CorsLayer::permissive()).with_state(state)
}
