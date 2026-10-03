use std::borrow::Cow;

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../dist"]
#[allow_missing = true]
struct Asset;

pub(super) async fn handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path();
    StaticFile(Cow::Owned(path.to_string()))
}

struct StaticFile(Cow<'static, str>);

impl IntoResponse for StaticFile {
    fn into_response(self) -> Response {
        let path = self.0.as_ref().trim_start_matches('/');

        'done: {
            if !path.is_empty() {
                let Some(content) = Asset::get(path) else {
                    break 'done;
                };

                let mime = mime_guess::from_path(path).first_or_octet_stream();
                return ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response();
            }
        };

        let Some(content) = Asset::get("index.html") else {
            return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
        };

        (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            content.data,
        )
            .into_response()
    }
}
