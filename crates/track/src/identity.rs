//! Who is making a request: a signed session cookie, or Cloudflare Access.

use std::sync::Arc;

use anyhow::Result;
use api::{Timestamp, UserId};
use auth::cloudflare::{Access, HttpKeyFetcher};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use cookie::Cookie;
use parking_lot::RwLock;
use tokio::sync::broadcast;

use crate::db::Database;
use crate::web::AppState;

pub(crate) const SESSION_COOKIE: &str = "track_session";

/// The authenticated user of a request or socket.
#[derive(Debug, Clone)]
pub(crate) struct AuthUser {
    pub(crate) id: UserId,
    /// The session this user signed in with, absent under Cloudflare Access.
    pub(crate) session: Option<String>,
}

/// Ends the sockets of a user or of one session.
#[derive(Debug, Clone)]
pub(crate) enum Revoke {
    User(UserId),
    Session(String),
}

impl Revoke {
    pub(crate) fn applies_to(&self, user: &AuthUser) -> bool {
        match self {
            Revoke::User(id) => *id == user.id,
            Revoke::Session(id) => user.session.as_deref() == Some(id.as_str()),
        }
    }
}

struct CloudflareState {
    config: api::CloudflareAccess,
    access: Option<Arc<Access>>,
}

struct Inner {
    key: [u8; 32],
    http: reqwest::Client,
    cloudflare: RwLock<CloudflareState>,
    revoke: broadcast::Sender<Revoke>,
}

#[derive(Clone)]
pub(crate) struct Auth {
    inner: Arc<Inner>,
}

impl Auth {
    pub(crate) fn new(key: [u8; 32], http: reqwest::Client, config: &api::Config) -> Self {
        let (revoke, _) = broadcast::channel(16);

        let auth = Self {
            inner: Arc::new(Inner {
                key,
                http,
                cloudflare: RwLock::new(CloudflareState {
                    config: api::CloudflareAccess::default(),
                    access: None,
                }),
                revoke,
            }),
        };

        auth.configure(config);
        auth
    }

    /// Applies changed Cloudflare Access settings.
    pub(crate) fn configure(&self, config: &api::Config) {
        let config = &config.cloudflare_access;
        let mut state = self.inner.cloudflare.write();

        if state.config == *config {
            return;
        }

        state.config = config.clone();

        state.access = config.enabled.then(|| {
            Arc::new(Access::with_fetcher(
                auth::cloudflare::Config {
                    team_domain: config.team_domain.trim().to_owned(),
                    audience: config.audience.trim().to_owned(),
                    trust_email_header: config.trust_email_header,
                    verify_jwt: config.verify_jwt,
                },
                HttpKeyFetcher::new(self.inner.http.clone()),
            ))
        });
    }

    /// Identifies the user of a request: a valid session cookie first, then
    /// Cloudflare Access, which only signs in an existing user by email.
    pub(crate) async fn authenticate(
        &self,
        db: &Database,
        headers: &HeaderMap,
    ) -> Result<Option<AuthUser>> {
        if let Some(session) = self.session_id(headers)
            && let Some(user) = db.session_user(&session, Timestamp::now()).await?
        {
            return Ok(Some(AuthUser {
                id: user.id,
                session: Some(session),
            }));
        }

        let access = self.inner.cloudflare.read().access.clone();

        let Some(access) = access else {
            return Ok(None);
        };

        let email = match access.email(headers).await {
            Ok(email) => email,
            Err(error) => {
                tracing::debug!(%error, "Cloudflare Access did not identify the request");
                return Ok(None);
            }
        };

        let Some(user) = db.user_by_email(&email).await? else {
            tracing::debug!(email, "No user has the Cloudflare Access email");
            return Ok(None);
        };

        Ok(Some(AuthUser {
            id: user.id,
            session: None,
        }))
    }

    /// The id of a correctly signed session cookie.
    pub(crate) fn session_id(&self, headers: &HeaderMap) -> Option<String> {
        headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(Cookie::split_parse)
            .filter_map(Result::ok)
            .filter(|c| c.name() == SESSION_COOKIE)
            .find_map(|c| {
                auth::verify_session_cookie(&self.inner.key, c.value()).map(str::to_owned)
            })
    }

    pub(crate) fn set_cookie(&self, session_id: &str) -> HeaderValue {
        let cookie = auth::session_cookie(SESSION_COOKIE, &self.inner.key, session_id);
        HeaderValue::try_from(cookie.to_string()).expect("session cookies are valid header values")
    }

    pub(crate) fn clear_cookie(&self) -> HeaderValue {
        let cookie = Cookie::build((SESSION_COOKIE, ""))
            .http_only(true)
            .path("/")
            .max_age(cookie::time::Duration::ZERO)
            .build();
        HeaderValue::try_from(cookie.to_string()).expect("session cookies are valid header values")
    }

    pub(crate) fn revoke(&self, revoke: Revoke) {
        _ = self.inner.revoke.send(revoke);
    }

    pub(crate) fn subscribe_revocations(&self) -> broadcast::Receiver<Revoke> {
        self.inner.revoke.subscribe()
    }
}

/// Rejects requests without a signed-in user.
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match state.auth.authenticate(&state.db, &parts.headers).await {
            Ok(Some(user)) => Ok(user),
            Ok(None) => Err(StatusCode::UNAUTHORIZED),
            Err(error) => {
                tracing::error!("Authenticating request: {error:#}");
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}
