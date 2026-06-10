use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use musli_web::ws::Channels;
use tokio::sync::Notify;
use tower_http::cors::CorsLayer;

use crate::app_broadcast::Broadcaster;
use crate::cache::ImageCache;
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::task_queue::TaskQueue;
use db::Database;

#[derive(Clone)]
pub(crate) struct AppState {
    pub db: Database,
    pub broadcast: Broadcaster,
    pub channels: Channels,
    pub cache: ImageCache,
    pub queue: TaskQueue,
    pub remote: RemoteClients,
    pub pending: PendingSystem,
    pub config_changed: Arc<Notify>,
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
