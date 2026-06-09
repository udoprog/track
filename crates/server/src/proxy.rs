use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::AppState;

pub(super) async fn image_handler(
    State(state): State<AppState>,
    Path((source, path)): Path<(String, String)>,
) -> Response {
    let url = match source.as_str() {
        "tmdb" => format!("https://image.tmdb.org/t/p/original/{path}"),
        "tvdb" => format!("https://artworks.thetvdb.com/{path}"),
        _ => return (StatusCode::BAD_REQUEST, "unknown image source").into_response(),
    };

    let http = state.http.clone();
    let cache = state.cache.clone();

    let result = cache
        .get_or_fetch(&source, &path, async || {
            let resp = http.get(&url).send().await?;
            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            let bytes = resp.error_for_status()?.bytes().await?;
            Ok(Some(bytes))
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
