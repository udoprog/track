use api::ImageSource;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;

use crate::identity::AuthUser;
use crate::web::AppState;

pub(super) async fn image_handler(
    State(state): State<AppState>,
    _user: AuthUser,
    Path((source, path)): Path<(ImageSource, String)>,
) -> Response {
    let cache = state.cache.clone();
    let remote = state.remote.clone();

    let path = path.trim_start_matches('/');

    let result = cache
        .get_or_fetch(source, path, async || match source {
            ImageSource::Tmdb => remote.fetch_tmdb_image(path).await,
            ImageSource::Tvdb => remote.fetch_tvdb_image(path).await,
            _ => Ok(None),
        })
        .await;

    match result {
        Ok(Some(data)) => image_response(path, data),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::warn!(%source, %path, error = %e, "Image proxy error");
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
