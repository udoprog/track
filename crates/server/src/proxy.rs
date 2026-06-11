use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::web::AppState;

pub(super) async fn image_handler(
    State(state): State<AppState>,
    Path((source, path)): Path<(String, String)>,
) -> Response {
    let cache = state.cache.clone();
    let remote = state.remote.clone();

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let result = cache
        .get_or_fetch(&source, &path, async || match source.as_str() {
            "tmdb" => remote.fetch_tmdb_image(&path).await,
            "tvdb" => remote.fetch_tvdb_image(&path).await,
            _ => anyhow::bail!("unknown image source: {source}"),
        })
        .await;

    match result {
        Ok(Some(data)) => image_response(&path, data),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::warn!(%source, %path, error = %e, "image proxy error");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

fn image_response(path: &str, data: Bytes) -> Response {
    let content_type = if path.ends_with(".png") {
        HeaderValue::from_static("image/png")
    } else if path.ends_with(".webp") {
        HeaderValue::from_static("image/webp")
    } else {
        HeaderValue::from_static("image/jpeg")
    };

    (
        [(header::CONTENT_TYPE, content_type)],
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=604800, immutable"),
        )],
        data,
    )
        .into_response()
}
