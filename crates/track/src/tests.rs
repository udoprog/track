//! Signing in, sessions, login links and request authorization, against a
//! server on a fresh database.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use api::Timestamp;
use musli_web::ws::Channels;
use reqwest::StatusCode;
use reqwest::header::{CONNECTION, COOKIE, HeaderMap, HeaderValue, SET_COOKIE, UPGRADE};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::{Notify, broadcast};

use crate::app_broadcast::Broadcaster;
use crate::cache::ImageCache;
use crate::db::{Database, OpenMode};
use crate::identity::{Auth, AuthUser, SESSION_COOKIE};
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;
use crate::web::{self, AppState};
use crate::ws::{Refused, WsHandler};

struct Server {
    url: String,
    state: AppState,
    client: reqwest::Client,
    _dir: TempDir,
}

impl Server {
    async fn start(config: api::Config) -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let db = Database::open(dir.path().join("track.db"), OpenMode::Bulk, 1)?;
        db.save_config(&config).await?;

        let http = reqwest::Client::new();
        let key = db.session_key().await?;
        let (tx, _) = broadcast::channel(16);

        let state = AppState {
            db: db.clone(),
            broadcast: Broadcaster::new(tx),
            channels: Channels::default(),
            cache: ImageCache::new(dir.path().join("image-cache")),
            queue: TaskQueue::new(),
            remote: RemoteClients::new(http.clone()),
            pending: PendingSystem::new(db),
            config_changed: Arc::new(Notify::new()),
            delay: None,
            shutdown: Shutdown::new(),
            auth: Auth::new(key, http, &config),
        };

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr: SocketAddr = listener.local_addr()?;
        let router = web::router(state.clone(), Some(dir.path()));
        tokio::spawn(async move { axum::serve(listener, router).await });

        Ok(Self {
            url: format!("http://{addr}"),
            state,
            client: reqwest::Client::new(),
            _dir: dir,
        })
    }

    async fn post(&self, path: &str, body: Value, cookie: Option<&str>) -> Result<Response> {
        let mut req = self
            .client
            .post(format!("{}{path}", self.url))
            .header("content-type", "application/json")
            .body(serde_json::to_vec(&body)?);

        if let Some(cookie) = cookie {
            req = req.header(COOKIE, cookie);
        }

        Response::from(req.send().await?).await
    }

    async fn get(&self, path: &str, headers: HeaderMap) -> Result<Response> {
        let req = self
            .client
            .get(format!("{}{path}", self.url))
            .headers(headers);
        Response::from(req.send().await?).await
    }

    async fn me(&self, cookie: &str) -> Result<Response> {
        self.get("/api/auth/me", cookie_header(cookie)).await
    }

    /// Signs in and returns the session cookie as a `Cookie` header value.
    async fn login(&self, login: &str, password: &str) -> Result<String> {
        let response = self
            .post(
                "/api/auth/login",
                json!({"login": login, "password": password}),
                None,
            )
            .await?;
        assert_eq!(response.status, StatusCode::OK);
        Ok(response.cookie.expect("login sets the session cookie"))
    }

    fn handler(&self, user: AuthUser) -> WsHandler {
        let state = &self.state;

        WsHandler {
            db: state.db.clone(),
            broadcast: state.broadcast.clone(),
            remote: state.remote.clone(),
            queue: state.queue.clone(),
            pending: state.pending.clone(),
            config_changed: state.config_changed.clone(),
            delay: None,
            auth: state.auth.clone(),
            user: Arc::new(user),
        }
    }

    async fn create_user(&self, login: &str, email: Option<&str>) -> Result<api::UserId> {
        let user = self
            .state
            .db
            .create_user(login, email, auth::UserRole::Regular, Timestamp::now())
            .await?
            .expect("login and email are free");
        Ok(user.id)
    }
}

struct Response {
    status: StatusCode,
    /// The session cookie set by the response, as `name=value`.
    cookie: Option<String>,
    body: Option<Value>,
}

impl Response {
    async fn from(response: reqwest::Response) -> Result<Self> {
        let status = response.status();

        let cookie = response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|v| cookie::Cookie::parse(v.to_str().ok()?.to_owned()).ok())
            .find(|c| c.name() == SESSION_COOKIE && !c.value().is_empty())
            .map(|c| format!("{}={}", c.name(), c.value()));

        let bytes = response.bytes().await?;
        let body = serde_json::from_slice(&bytes).ok();

        Ok(Self {
            status,
            cookie,
            body,
        })
    }
}

fn cookie_header(cookie: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, HeaderValue::from_str(cookie).unwrap());
    headers
}

fn upgrade_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONNECTION, HeaderValue::from_static("upgrade"));
    headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
    headers.insert("sec-websocket-version", HeaderValue::from_static("13"));
    headers.insert(
        "sec-websocket-key",
        HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
    );
    headers
}

fn refusal(error: anyhow::Error) -> Refused {
    *error
        .downcast_ref::<Refused>()
        .unwrap_or_else(|| panic!("expected a refusal, got {error:#}"))
}

#[tokio::test]
async fn login_and_logout() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;

    let cookie = server.login("root", "root").await?;

    let me = server.me(&cookie).await?;
    assert_eq!(me.status, StatusCode::OK);
    let me = me.body.unwrap();
    assert_eq!(me["login"], "root");
    assert_eq!(me["role"], "admin");

    let logout = server
        .post("/api/auth/logout", json!({}), Some(&cookie))
        .await?;
    assert_eq!(logout.status, StatusCode::NO_CONTENT);

    assert_eq!(server.me(&cookie).await?.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn login_by_email() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    let id = server
        .create_user("alice", Some("alice@example.com"))
        .await?;
    let hash = auth::hash_password("alice password")?;
    server.state.db.set_user_password_hash(id, &hash).await?;

    let cookie = server.login("Alice@Example.com", "alice password").await?;
    let me = server.me(&cookie).await?;
    assert_eq!(me.body.unwrap()["login"], "alice");
    Ok(())
}

#[tokio::test]
async fn bad_password() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;

    for (login, password) in [("root", "wrong"), ("nobody", "root"), ("root", "")] {
        let response = server
            .post(
                "/api/auth/login",
                json!({"login": login, "password": password}),
                None,
            )
            .await?;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED, "{login}");
        assert!(response.cookie.is_none());
    }

    // A user without a password cannot sign in with one.
    server.create_user("bob", None).await?;
    let response = server
        .post(
            "/api/auth/login",
            json!({"login": "bob", "password": ""}),
            None,
        )
        .await?;
    assert_eq!(response.status, StatusCode::UNAUTHORIZED);

    // A forged cookie is not a session.
    let forged = format!("{SESSION_COOKIE}=abc.def");
    assert_eq!(server.me(&forged).await?.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;

    let ws = server.get("/ws", upgrade_headers()).await?;
    assert_eq!(ws.status, StatusCode::UNAUTHORIZED);

    let image = server
        .get("/api/image/tmdb/poster.jpg", HeaderMap::new())
        .await?;
    assert_eq!(image.status, StatusCode::UNAUTHORIZED);

    let me = server.get("/api/auth/me", HeaderMap::new()).await?;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);

    // The SPA stays public.
    let index = server.get("/", HeaderMap::new()).await?;
    assert_ne!(index.status, StatusCode::UNAUTHORIZED);

    let cookie = server.login("root", "root").await?;
    let mut headers = upgrade_headers();
    headers.extend(cookie_header(&cookie));
    let ws = server.get("/ws", headers).await?;
    assert_eq!(ws.status, StatusCode::SWITCHING_PROTOCOLS);
    Ok(())
}

#[tokio::test]
async fn admin_only_requests() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    let root = server
        .state
        .db
        .user_by_login_or_email("root")
        .await?
        .unwrap();
    let alice = server.create_user("alice", None).await?;

    let regular = server.handler(AuthUser {
        id: alice,
        session: None,
    });

    for id in [
        api::Request::SetConfig,
        api::Request::ListUsers,
        api::Request::CreateUser,
        api::Request::SetUserRole,
        api::Request::DeleteUser,
        api::Request::GenerateLoginToken,
        api::Request::RevokeLoginToken,
        api::Request::RevokeUserAccess,
    ] {
        let error = regular.authorize(id).await.unwrap_err();
        assert_eq!(refusal(error), Refused::NotAdmin, "{id:?}");
    }

    for id in [
        api::Request::GetConfig,
        api::Request::ListMedia,
        api::Request::SyncAll,
        api::Request::SetLogin,
        api::Request::SetEmail,
        api::Request::SetPassword,
    ] {
        regular.authorize(id).await?;
    }

    let admin = server.handler(AuthUser {
        id: root.id,
        session: None,
    });
    admin.authorize(api::Request::SetConfig).await?;
    admin.authorize(api::Request::ListUsers).await?;

    // A role change applies to open sockets.
    server
        .state
        .db
        .set_user_role(alice, auth::UserRole::Admin)
        .await?;
    regular.authorize(api::Request::ListUsers).await?;
    Ok(())
}

#[tokio::test]
async fn login_token_is_single_use() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    let db = &server.state.db;
    let alice = server
        .create_user("alice", Some("alice@example.com"))
        .await?;
    let expires = Timestamp::from_jiff(auth::login_token_expiry(Timestamp::now().into_jiff()));

    db.create_login_token("first", alice, expires).await?;
    // A newer link replaces the unused one.
    db.create_login_token("second", alice, expires).await?;

    let first = server.get("/api/register/first", HeaderMap::new()).await?;
    assert_eq!(first.status, StatusCode::NOT_FOUND);

    let info = server.get("/api/register/second", HeaderMap::new()).await?;
    assert_eq!(info.status, StatusCode::OK);
    let info = info.body.unwrap();
    assert_eq!(info["login"], "alice");
    assert_eq!(info["email"], "alice@example.com");

    let weak = server
        .post("/api/register/second", json!({"password": "short"}), None)
        .await?;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);

    let register = server
        .post(
            "/api/register/second",
            json!({"password": "new password"}),
            None,
        )
        .await?;
    assert_eq!(register.status, StatusCode::OK);
    let cookie = register.cookie.expect("registering signs in");
    assert_eq!(server.me(&cookie).await?.body.unwrap()["login"], "alice");

    let again = server
        .post(
            "/api/register/second",
            json!({"password": "other password"}),
            None,
        )
        .await?;
    assert_eq!(again.status, StatusCode::GONE);
    assert_eq!(
        server
            .get("/api/register/second", HeaderMap::new())
            .await?
            .status,
        StatusCode::GONE
    );

    server.login("alice", "new password").await?;
    Ok(())
}

#[tokio::test]
async fn login_token_expires() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    let alice = server.create_user("alice", None).await?;
    let expired =
        Timestamp::from_jiff(Timestamp::now().into_jiff() - jiff::SignedDuration::from_secs(1));

    server
        .state
        .db
        .create_login_token("old", alice, expired)
        .await?;

    let info = server.get("/api/register/old", HeaderMap::new()).await?;
    assert_eq!(info.status, StatusCode::GONE);

    let register = server
        .post(
            "/api/register/old",
            json!({"password": "new password"}),
            None,
        )
        .await?;
    assert_eq!(register.status, StatusCode::GONE);
    assert!(register.cookie.is_none());
    Ok(())
}

#[tokio::test]
async fn cloudflare_matches_email_only() -> Result<()> {
    let config = api::Config {
        cloudflare_access: api::CloudflareAccess {
            enabled: true,
            team_domain: "example.cloudflareaccess.com".to_owned(),
            audience: "aud".to_owned(),
            trust_email_header: true,
            verify_jwt: false,
        },
        ..api::Config::default()
    };

    let server = Server::start(config).await?;
    server
        .create_user("alice", Some("alice@example.com"))
        .await?;

    let email = |email: &'static str| {
        let mut headers = HeaderMap::new();
        headers.insert(
            auth::cloudflare::EMAIL_HEADER,
            HeaderValue::from_static(email),
        );
        headers
    };

    let me = server
        .get("/api/auth/me", email("Alice@Example.com"))
        .await?;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body.unwrap()["login"], "alice");

    // A login name is not an email, and unknown emails create no user.
    for unknown in ["root", "alice", "carol@example.com"] {
        let me = server.get("/api/auth/me", email(unknown)).await?;
        assert_eq!(me.status, StatusCode::UNAUTHORIZED, "{unknown}");
    }

    assert_eq!(server.state.db.list_users().await?.len(), 2);

    let ws = server.get("/ws", {
        let mut headers = upgrade_headers();
        headers.extend(email("alice@example.com"));
        headers
    });
    assert_eq!(ws.await?.status, StatusCode::SWITCHING_PROTOCOLS);
    Ok(())
}
