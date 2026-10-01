//! Signing in and out, and redeeming login links, over plain HTTP: the
//! websocket only opens once there is a session.

use gloo::net::http::{Request, Response};
use serde::{Deserialize, Serialize};

/// Why an auth request failed, by the server's status.
#[derive(Debug)]
pub(crate) enum HttpError {
    /// No session, or a wrong login or password.
    Unauthorized,
    /// No such login link.
    NotFound,
    /// The login link was used or has expired.
    Gone,
    /// The server refused the input and says why.
    BadRequest(String),
    Status(u16),
    /// The server could not be reached or answered with something unreadable.
    Request(gloo::net::Error),
}

impl From<gloo::net::Error> for HttpError {
    fn from(error: gloo::net::Error) -> Self {
        Self::Request(error)
    }
}

/// What a login link is for.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct RegisterInfo {
    pub(crate) login: String,
    pub(crate) email: Option<String>,
}

#[derive(Serialize)]
struct LoginBody<'a> {
    login: &'a str,
    password: &'a str,
}

#[derive(Serialize)]
struct RegisterBody<'a> {
    password: &'a str,
}

async fn check(response: Response) -> Result<Response, HttpError> {
    match response.status() {
        200..=299 => Ok(response),
        400 => Err(HttpError::BadRequest(response.text().await?)),
        401 => Err(HttpError::Unauthorized),
        404 => Err(HttpError::NotFound),
        410 => Err(HttpError::Gone),
        status => Err(HttpError::Status(status)),
    }
}

/// The signed-in user.
pub(crate) async fn me() -> Result<api::User, HttpError> {
    let response = Request::get("/api/auth/me").send().await?;
    Ok(check(response).await?.json().await?)
}

/// Signs in with a login name or email.
pub(crate) async fn login(login: &str, password: &str) -> Result<api::User, HttpError> {
    let response = Request::post("/api/auth/login")
        .json(&LoginBody { login, password })?
        .send()
        .await?;

    Ok(check(response).await?.json().await?)
}

pub(crate) async fn logout() -> Result<(), HttpError> {
    let response = Request::post("/api/auth/logout").send().await?;
    check(response).await?;
    Ok(())
}

pub(crate) async fn register_info(token: &str) -> Result<RegisterInfo, HttpError> {
    let response = Request::get(&format!("/api/register/{token}"))
        .send()
        .await?;

    Ok(check(response).await?.json().await?)
}

/// Sets the password of a login link's user and signs them in.
pub(crate) async fn register(token: &str, password: &str) -> Result<api::User, HttpError> {
    let response = Request::post(&format!("/api/register/{token}"))
        .json(&RegisterBody { password })?
        .send()
        .await?;

    Ok(check(response).await?.json().await?)
}
