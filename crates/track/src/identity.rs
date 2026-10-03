//! Session authentication and Cloudflare Access sign-in eligibility.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use api::{Timestamp, UserId};
use auth::cloudflare::{Access, HttpKeyFetcher};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use cookie::Cookie;
use parking_lot::{Mutex, RwLock};
use tokio::sync::broadcast;

use crate::db::Database;
use crate::login_throttle::LoginThrottle;
use crate::web::AppState;

pub(crate) const SESSION_COOKIE: &str = "track_session";

/// The authenticated user of a request or socket.
#[derive(Debug, Clone)]
pub(crate) struct AuthUser {
    pub(crate) id: UserId,
    /// The session this user signed in with.
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
    /// When each Access warning was last logged, so a client retrying every
    /// few seconds does not flood the log.
    warned: Mutex<HashMap<String, Instant>>,
    throttle: LoginThrottle,
}

/// How often the same Access warning may be logged.
const WARN_INTERVAL: Duration = Duration::from_secs(60);

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
                warned: Mutex::new(HashMap::new()),
                throttle: LoginThrottle::default(),
            }),
        };

        auth.apply(config, true);
        auth
    }

    /// Applies changed Cloudflare Access settings.
    pub(crate) fn configure(&self, config: &api::Config) {
        self.apply(config, false);
    }

    fn apply(&self, config: &api::Config, initial: bool) {
        let config = &config.cloudflare_access;
        let mut state = self.inner.cloudflare.write();

        if !initial && state.config == *config {
            return;
        }

        state.config = config.clone();
        log_access_settings(config);

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

    /// Identifies the user of a request by its session cookie.
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

        Ok(None)
    }

    pub(crate) async fn cloudflare_user(
        &self,
        db: &Database,
        headers: &HeaderMap,
    ) -> Result<Option<crate::db::users::UserRecord>> {
        let access = self.inner.cloudflare.read().access.clone();

        // Without anything from Access the request is simply signed out, which
        // is not worth a warning.
        let presented = presents_access(headers);

        let Some(access) = access else {
            if presented {
                self.warn("Cloudflare Access sign-in is disabled in Settings, so its credentials on this request were ignored".to_owned());
            }

            return Ok(None);
        };

        let email = match access.email(headers).await {
            Ok(email) => email,
            Err(error) if presented => {
                self.warn(format!(
                    "Cloudflare Access sign-in failed: {}",
                    chain(&error)
                ));
                return Ok(None);
            }
            Err(error) => {
                tracing::debug!(%error, "Cloudflare Access did not identify the request");
                return Ok(None);
            }
        };

        let Some(user) = db.user_by_email(&email).await? else {
            self.warn(format!(
                "Cloudflare Access sign-in failed: no user has the email {email}"
            ));
            return Ok(None);
        };

        Ok(Some(user))
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

    /// Logs an Access warning unless the same one was logged recently.
    fn warn(&self, message: String) {
        let now = Instant::now();
        let mut warned = self.inner.warned.lock();
        warned.retain(|_, at| now.duration_since(*at) < WARN_INTERVAL);

        if warned.contains_key(&message) {
            return;
        }

        tracing::warn!("{message}");
        warned.insert(message, now);
    }

    /// The Access warnings logged within the last minute.
    #[cfg(test)]
    pub(crate) fn warnings(&self) -> Vec<String> {
        self.inner.warned.lock().keys().cloned().collect()
    }

    pub(crate) fn throttle(&self) -> &LoginThrottle {
        &self.inner.throttle
    }

    pub(crate) fn revoke(&self, revoke: Revoke) {
        _ = self.inner.revoke.send(revoke);
    }

    pub(crate) fn subscribe_revocations(&self) -> broadcast::Receiver<Revoke> {
        self.inner.revoke.subscribe()
    }
}

fn log_access_settings(config: &api::CloudflareAccess) {
    if !config.enabled {
        tracing::info!("Cloudflare Access sign-in is disabled");
        return;
    }

    if config.team_domain.trim().is_empty() || config.audience.trim().is_empty() {
        tracing::warn!(
            "Cloudflare Access sign-in is enabled, but its team domain or audience is empty, so no one can sign in through it"
        );
        return;
    }

    tracing::info!(
        team_domain = config.team_domain.trim(),
        verify_jwt = config.verify_jwt,
        trust_email_header = config.trust_email_header,
        "Cloudflare Access sign-in is enabled"
    );
}

/// Whether the request carries anything Cloudflare Access adds: its email
/// header, its token header or its cookie.
fn presents_access(headers: &HeaderMap) -> bool {
    use auth::cloudflare::{EMAIL_HEADER, JWT_COOKIE, JWT_HEADER};

    headers.contains_key(EMAIL_HEADER)
        || headers.contains_key(JWT_HEADER)
        || headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(Cookie::split_parse)
            .filter_map(Result::ok)
            .any(|c| c.name() == JWT_COOKIE)
}

/// An error with its causes, such as why the signing keys could not be fetched.
fn chain(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();

    while let Some(error) = source {
        out.push_str(": ");
        out.push_str(&error.to_string());
        source = error.source();
    }

    out
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
