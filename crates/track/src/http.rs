//! Signing in and out, and redeeming login links.

use std::net::SocketAddr;
use std::time::Instant;

use api::Timestamp;
use axum::Json;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use tokio::task::spawn_blocking;

use crate::db::users::TokenError;
use crate::identity::{AuthUser, Revoke};
use crate::web::AppState;

pub(crate) enum Error {
    Unauthorized,
    TooManyRequests,
    NotFound,
    /// The login link was used or has expired.
    Gone,
    BadRequest(&'static str),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl From<TokenError> for Error {
    fn from(error: TokenError) -> Self {
        match error {
            TokenError::NotFound => Self::NotFound,
            TokenError::Gone => Self::Gone,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED.into_response(),
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS.into_response(),
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
            Self::Gone => StatusCode::GONE.into_response(),
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message).into_response(),
            Self::Internal(error) => {
                tracing::error!("{error:#}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LoginBody {
    /// A login name or an email.
    login: String,
    password: String,
}

#[derive(Deserialize)]
pub(crate) struct RegisterBody {
    password: String,
}

#[derive(Serialize)]
pub(crate) struct RegisterInfo {
    login: String,
    email: Option<String>,
}

pub(crate) async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(body): Json<LoginBody>,
) -> Result<Response, Error> {
    let throttle = state.auth.throttle();

    if !throttle.allows(peer.ip(), &body.login, Instant::now()) {
        return Err(Error::TooManyRequests);
    }

    let user = state.db.user_by_login_or_email(&body.login).await?;

    // An unknown login or a user without a password is checked against a
    // dummy hash, so the response time does not tell whether the account exists.
    let hash = user.as_ref().and_then(|u| u.password_hash.clone());
    let password = body.password;

    let verified = spawn_blocking(move || match &hash {
        Some(hash) => auth::verify_password(&password, hash),
        None => auth::verify_password(&password, auth::dummy_hash()),
    })
    .await
    .map_err(anyhow::Error::from)?;

    let Some(user) = user.filter(|_| verified) else {
        throttle.fail(peer.ip(), &body.login, Instant::now());
        return Err(Error::Unauthorized);
    };

    throttle.succeed(&body.login);

    let session_id = auth::new_session_id();
    state
        .db
        .create_session(&session_id, user.id, Timestamp::now())
        .await?;

    Ok((
        [(header::SET_COOKIE, state.auth.set_cookie(&session_id))],
        Json(user.to_api()),
    )
        .into_response())
}

pub(crate) async fn cloudflare_available(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<bool>, Error> {
    Ok(Json(
        state
            .auth
            .cloudflare_user(&state.db, &headers)
            .await?
            .is_some(),
    ))
}

pub(crate) async fn cloudflare_login(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Error> {
    let user = state
        .auth
        .cloudflare_user(&state.db, &headers)
        .await?
        .ok_or(Error::Unauthorized)?;
    let session_id = auth::new_session_id();
    state
        .db
        .create_session(&session_id, user.id, Timestamp::now())
        .await?;
    Ok((
        [(header::SET_COOKIE, state.auth.set_cookie(&session_id))],
        Json(user.to_api()),
    )
        .into_response())
}

pub(crate) async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(session_id) = state.auth.session_id(&headers) {
        if let Err(error) = state.db.delete_session(&session_id).await {
            tracing::error!("Deleting session: {error:#}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }

        state.auth.revoke(Revoke::Session(session_id));
    }

    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, state.auth.clear_cookie())],
    )
        .into_response()
}

pub(crate) async fn me(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<api::User>, Error> {
    let user = state
        .db
        .user_by_id(user.id)
        .await?
        .ok_or(Error::Unauthorized)?;

    Ok(Json(user.to_api()))
}

pub(crate) async fn get_register(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<RegisterInfo>, Error> {
    let user = state
        .db
        .login_token_user(&token, Timestamp::now())
        .await??;

    Ok(Json(RegisterInfo {
        login: user.login,
        email: user.email,
    }))
}

pub(crate) async fn post_register(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<RegisterBody>,
) -> Result<Response, Error> {
    if let Some(message) = auth::validate_password(&body.password) {
        return Err(Error::BadRequest(message));
    }

    // Checked before hashing, so an invalid link costs no hashing work.
    state
        .db
        .login_token_user(&token, Timestamp::now())
        .await??;

    let hash = spawn_blocking(move || auth::hash_password(&body.password))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(anyhow::Error::from)?;
    let session_id = auth::new_session_id();

    let user = state
        .db
        .redeem_login_token(&token, &hash, &session_id, Timestamp::now())
        .await??;

    state.auth.revoke(Revoke::User(user.id));

    Ok((
        [(header::SET_COOKIE, state.auth.set_cookie(&session_id))],
        Json(user.to_api()),
    )
        .into_response())
}
