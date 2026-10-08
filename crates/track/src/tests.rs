//! Signing in, sessions, login links and request authorization, against a
//! server on a fresh database.

use std::net::{IpAddr, SocketAddr};
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
use crate::identity::{Auth, AuthUser, Revoke, SESSION_COOKIE};
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;
use crate::web::{self, AppState};
use crate::ws::{Refused, WsHandler, session_valid};

struct Server {
    url: String,
    state: AppState,
    client: reqwest::Client,
    _dir: TempDir,
}

impl Server {
    async fn start(config: api::Config) -> Result<Self> {
        Self::start_trusting(config, &[]).await
    }

    async fn start_trusting(config: api::Config, trusted_proxies: &[IpAddr]) -> Result<Self> {
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
            remote: RemoteClients::new(http.clone(), http.clone()),
            pending: PendingSystem::new(db),
            config_changed: Arc::new(Notify::new()),
            delay: None,
            shutdown: Shutdown::new(),
            auth: Auth::new(key, http, &config, trusted_proxies),
        };

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr: SocketAddr = listener.local_addr()?;
        let router = web::router(state.clone(), Some(dir.path()));
        tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
        });

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
    server
        .state
        .db
        .set_user_password_hash(id, &hash, None)
        .await?;

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
async fn session_cookie_is_secure_behind_https() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;

    let set_cookies = |response: &reqwest::Response| -> Vec<cookie::Cookie<'static>> {
        response
            .headers()
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|v| cookie::Cookie::parse(v.to_str().ok()?.to_owned()).ok())
            .collect()
    };

    for (proto, secure) in [
        (None, None),
        (Some("http"), None),
        (Some("https"), Some(true)),
    ] {
        let mut req = server
            .client
            .post(format!("{}/api/auth/login", server.url))
            .header("content-type", "application/json")
            .body(serde_json::to_vec(
                &json!({"login": "root", "password": "root"}),
            )?);

        if let Some(proto) = proto {
            req = req.header("x-forwarded-proto", proto);
        }

        let response = req.send().await?;
        assert_eq!(response.status(), StatusCode::OK);
        let cookies = set_cookies(&response);
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].secure(), secure, "{proto:?}");

        let mut req = server
            .client
            .post(format!("{}/api/auth/logout", server.url))
            .header(
                COOKIE,
                format!("{}={}", cookies[0].name(), cookies[0].value()),
            );

        if let Some(proto) = proto {
            req = req.header("x-forwarded-proto", proto);
        }

        let cleared = set_cookies(&req.send().await?);
        assert_eq!(cleared.len(), 1);
        assert_eq!(cleared[0].value(), "");
        assert_eq!(cleared[0].same_site(), Some(cookie::SameSite::Strict));
        assert_eq!(cleared[0].secure(), secure, "{proto:?}");
    }

    let root = server
        .state
        .db
        .user_by_login_or_email("root")
        .await?
        .unwrap();
    let hash = root.password_hash.as_deref().unwrap();
    assert!(!format!("{root:?}").contains(hash));
    Ok(())
}

#[tokio::test]
async fn failed_logins_are_throttled() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;

    let mut statuses = Vec::new();

    for _ in 0..11 {
        let response = server
            .post(
                "/api/auth/login",
                json!({"login": "root", "password": "wrong"}),
                None,
            )
            .await?;
        statuses.push(response.status);
    }

    assert!(
        statuses[..10]
            .iter()
            .all(|s| *s == StatusCode::UNAUTHORIZED)
    );
    assert_eq!(statuses[10], StatusCode::TOO_MANY_REQUESTS);

    // Even the right password is refused while the login is throttled.
    let response = server
        .post(
            "/api/auth/login",
            json!({"login": "root", "password": "root"}),
            None,
        )
        .await?;
    assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(response.cookie.is_none());
    Ok(())
}

/// Fails 30 sign-ins as distinct logins, each claiming a distinct forwarded
/// address, then returns the status of one more.
async fn forwarded_attempts(server: &Server) -> Result<StatusCode> {
    let mut status = StatusCode::OK;

    for i in 0..=30 {
        let response = server
            .client
            .post(format!("{}/api/auth/login", server.url))
            .header("content-type", "application/json")
            .header("x-forwarded-for", format!("198.51.100.{i}"))
            .body(serde_json::to_vec(
                &json!({"login": format!("user{i}"), "password": "wrong"}),
            )?)
            .send()
            .await?;
        status = response.status();
    }

    Ok(status)
}

#[tokio::test]
async fn forwarded_addresses_need_a_trusted_proxy() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    assert_eq!(
        forwarded_attempts(&server).await?,
        StatusCode::TOO_MANY_REQUESTS
    );

    let server = Server::start_trusting(api::Config::default(), &["127.0.0.1".parse()?]).await?;
    assert_eq!(forwarded_attempts(&server).await?, StatusCode::UNAUTHORIZED);
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
        api::Request::GetSystemConfig,
        api::Request::SetSystemConfig,
        api::Request::RemoveShow,
        api::Request::RemoveMovie,
        api::Request::DeletePerson,
        api::Request::ListUsers,
        api::Request::CreateUser,
        api::Request::SetUserRole,
        api::Request::DeleteUser,
        api::Request::GenerateLoginToken,
        api::Request::RevokeLoginToken,
        api::Request::RevokeUserAccess,
        api::Request::AddShowRemote,
        api::Request::RemoveShowRemote,
        api::Request::UpdateShowRemote,
        api::Request::SetShowRemoteEnabled,
        api::Request::SetShowRemoteSyncKinds,
        api::Request::ReorderShowRemotes,
        api::Request::PurgeShowRemoteCache,
        api::Request::PurgeEpisodeCache,
        api::Request::SetShowAutoSync,
        api::Request::SetShowAirDateFilters,
        api::Request::SetShowNumbering,
        api::Request::AddMovieRemote,
        api::Request::RemoveMovieRemote,
        api::Request::UpdateMovieRemote,
        api::Request::SetMovieRemoteEnabled,
        api::Request::SetMovieRemoteSyncKinds,
        api::Request::ReorderMovieRemotes,
        api::Request::PurgeMovieRemoteCache,
        api::Request::SetMovieAutoSync,
        api::Request::SetMovieReleaseFilters,
        api::Request::AddPersonRemote,
        api::Request::RemovePersonRemote,
        api::Request::UpdatePersonRemote,
        api::Request::SetPersonRemoteEnabled,
        api::Request::SetPersonRemoteSyncKinds,
        api::Request::ReorderPersonRemotes,
        api::Request::PurgePersonRemoteCache,
        api::Request::SelectImage,
        api::Request::ClearSelectedImage,
        api::Request::PickBestImages,
        api::Request::ResetImageSelection,
        api::Request::SyncAll,
        api::Request::RemoveTask,
        api::Request::BumpTask,
    ] {
        let error = regular.authorize(id).await.unwrap_err();
        assert_eq!(refusal(error), Refused::NotAdmin, "{id:?}");
    }

    for id in [
        api::Request::GetPreferences,
        api::Request::SetPreferences,
        api::Request::ListMedia,
        api::Request::TrackShow,
        api::Request::SyncShow,
        api::Request::ListTasks,
        api::Request::SetShowLanguage,
        api::Request::GetShowNumbering,
        api::Request::MarkWatched,
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
    admin.authorize(api::Request::SetSystemConfig).await?;
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

    let pending = db.pending_login_links(Timestamp::now()).await?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].user_id, alice);

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

    assert!(db.pending_login_links(Timestamp::now()).await?.is_empty());

    server.login("alice", "new password").await?;
    Ok(())
}

#[tokio::test]
async fn password_change_ends_other_sessions() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    let db = &server.state.db;
    let root = db.user_by_login_or_email("root").await?.unwrap();

    let current = server.login("root", "root").await?;
    let other = server.login("root", "root").await?;
    let current_id = server.state.auth.session_id(&cookie_header(&current));
    let other_id = server.state.auth.session_id(&cookie_header(&other));

    let hash = auth::hash_password("new password")?;
    db.set_user_password_hash(root.id, &hash, current_id.as_deref())
        .await?;

    assert_eq!(server.me(&current).await?.status, StatusCode::OK);
    assert_eq!(server.me(&other).await?.status, StatusCode::UNAUTHORIZED);

    let socket = |session: &Option<String>| AuthUser {
        id: root.id,
        session: session.clone(),
    };

    // Open sockets of the ended session close on the revocation, or on their
    // next check if they missed it.
    let revoke = Revoke::OtherSessions {
        user: root.id,
        keep: current_id.clone(),
    };
    assert!(!revoke.applies_to(&socket(&current_id)));
    assert!(revoke.applies_to(&socket(&other_id)));
    assert!(session_valid(db, &socket(&current_id)).await);
    assert!(!session_valid(db, &socket(&other_id)).await);
    Ok(())
}

#[tokio::test]
async fn login_link_ends_existing_sessions() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    let db = &server.state.db;
    let alice = server.create_user("alice", None).await?;
    let hash = auth::hash_password("old password")?;
    db.set_user_password_hash(alice, &hash, None).await?;
    let old = server.login("alice", "old password").await?;

    let expires = Timestamp::from_jiff(auth::login_token_expiry(Timestamp::now().into_jiff()));
    db.create_login_token("reset", alice, expires).await?;

    let register = server
        .post(
            "/api/register/reset",
            json!({"password": "new password"}),
            None,
        )
        .await?;
    assert_eq!(register.status, StatusCode::OK);
    let new = register.cookie.expect("registering signs in");

    assert_eq!(server.me(&old).await?.status, StatusCode::UNAUTHORIZED);
    assert_eq!(server.me(&new).await?.status, StatusCode::OK);
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

    assert!(
        server
            .state
            .db
            .pending_login_links(Timestamp::now())
            .await?
            .is_empty()
    );

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

    let available = server
        .get("/api/auth/cloudflare", email("Alice@Example.com"))
        .await?;
    assert_eq!(available.body, Some(json!(true)));
    assert!(available.cookie.is_none());
    assert_eq!(
        server
            .get("/api/auth/me", email("alice@example.com"))
            .await?
            .status,
        StatusCode::UNAUTHORIZED
    );
    let mut upgrade = upgrade_headers();
    upgrade.extend(email("alice@example.com"));
    assert_eq!(
        server.get("/ws", upgrade).await?.status,
        StatusCode::UNAUTHORIZED
    );

    for headers in [
        HeaderMap::new(),
        email("root"),
        email("alice"),
        email("carol@example.com"),
    ] {
        let available = server.get("/api/auth/cloudflare", headers.clone()).await?;
        assert_eq!(available.body, Some(json!(false)));
        let login = Response::from(
            server
                .client
                .post(format!("{}/api/auth/cloudflare", server.url))
                .headers(headers)
                .send()
                .await?,
        )
        .await?;
        assert_eq!(login.status, StatusCode::UNAUTHORIZED);
        assert!(login.cookie.is_none());
    }

    let login = Response::from(
        server
            .client
            .post(format!("{}/api/auth/cloudflare", server.url))
            .headers(email("Alice@Example.com"))
            .send()
            .await?,
    )
    .await?;
    assert_eq!(login.status, StatusCode::OK);
    assert_eq!(login.body.unwrap()["login"], "alice");
    let cookie = login.cookie.expect("Cloudflare sign-in creates a session");
    assert_eq!(server.me(&cookie).await?.body.unwrap()["login"], "alice");
    let mut upgrade = upgrade_headers();
    upgrade.extend(cookie_header(&cookie));
    assert_eq!(
        server.get("/ws", upgrade).await?.status,
        StatusCode::SWITCHING_PROTOCOLS
    );

    let root = server.login("root", "root").await?;
    let mut headers = email("alice@example.com");
    headers.extend(cookie_header(&root));
    assert_eq!(
        server.get("/api/auth/me", headers).await?.body.unwrap()["login"],
        "root"
    );

    let mut headers = email("alice@example.com");
    headers.extend(cookie_header(&cookie));
    let logout = server
        .client
        .post(format!("{}/api/auth/logout", server.url))
        .headers(headers.clone())
        .send()
        .await?;
    assert_eq!(logout.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        server.get("/api/auth/me", headers).await?.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .get("/api/auth/cloudflare", email("alice@example.com"))
            .await?
            .body,
        Some(json!(true))
    );
    assert_eq!(server.state.db.list_users().await?.len(), 2);
    Ok(())
}

#[tokio::test]
async fn cloudflare_rejects_disabled_and_invalid_credentials() -> Result<()> {
    for enabled in [false, true] {
        let server = Server::start(api::Config {
            cloudflare_access: api::CloudflareAccess {
                enabled,
                team_domain: "example.cloudflareaccess.com".to_owned(),
                audience: "aud".to_owned(),
                trust_email_header: true,
                verify_jwt: true,
            },
            ..api::Config::default()
        })
        .await?;
        server
            .create_user("alice", Some("alice@example.com"))
            .await?;
        let mut headers = HeaderMap::new();
        headers.insert(
            auth::cloudflare::EMAIL_HEADER,
            HeaderValue::from_static("alice@example.com"),
        );
        headers.insert(
            auth::cloudflare::JWT_HEADER,
            HeaderValue::from_static("a.b.c"),
        );
        assert_eq!(
            server
                .get("/api/auth/cloudflare", headers.clone())
                .await?
                .body,
            Some(json!(false))
        );
        let response = Response::from(
            server
                .client
                .post(format!("{}/api/auth/cloudflare", server.url))
                .headers(headers.clone())
                .send()
                .await?,
        )
        .await?;
        assert_eq!(response.status, StatusCode::UNAUTHORIZED);
        assert!(response.cookie.is_none());
        assert_eq!(
            server.get("/api/auth/me", headers).await?.status,
            StatusCode::UNAUTHORIZED
        );
    }
    Ok(())
}

/// Loading the sign-in page with Cloudflare Access credentials warns why they
/// did not sign anyone in, once per minute; a plain signed-out request warns
/// about nothing.
#[tokio::test]
async fn access_failures_are_logged() -> Result<()> {
    let cookie = |value: &'static str| {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static(value));
        headers
    };

    // Disabled: the credentials are ignored, and the log says so.
    let server = Server::start(api::Config::default()).await?;

    let me = server.get("/api/auth/cloudflare", HeaderMap::new()).await?;
    assert_eq!(me.body, Some(json!(false)));
    assert!(server.state.auth.warnings().is_empty());

    for _ in 0..2 {
        let me = server
            .get("/api/auth/cloudflare", cookie("CF_Authorization=a.b.c"))
            .await?;
        assert_eq!(me.body, Some(json!(false)));
    }

    let warnings = server.state.auth.warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("sign-in is disabled in Settings"));

    // Enabled: a token that does not verify, and an email no one has.
    let server = Server::start(api::Config {
        cloudflare_access: api::CloudflareAccess {
            enabled: true,
            team_domain: "example.cloudflareaccess.com".to_owned(),
            audience: "aud".to_owned(),
            trust_email_header: true,
            verify_jwt: false,
        },
        ..api::Config::default()
    })
    .await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        auth::cloudflare::EMAIL_HEADER,
        HeaderValue::from_static("carol@example.com"),
    );
    server.get("/api/auth/cloudflare", headers).await?;
    assert!(
        server
            .state
            .auth
            .warnings()
            .iter()
            .any(|w| w.contains("no user has the email carol@example.com")),
        "{:?}",
        server.state.auth.warnings()
    );

    let server = Server::start(api::Config {
        cloudflare_access: api::CloudflareAccess {
            enabled: true,
            team_domain: "example.cloudflareaccess.com".to_owned(),
            audience: "aud".to_owned(),
            trust_email_header: false,
            verify_jwt: true,
        },
        ..api::Config::default()
    })
    .await?;
    server
        .get("/api/auth/cloudflare", cookie("CF_Authorization=a.b.c"))
        .await?;
    assert!(
        server
            .state
            .auth
            .warnings()
            .iter()
            .any(|w| w.contains("Cloudflare Access sign-in failed: malformed Access JWT")),
        "{:?}",
        server.state.auth.warnings()
    );
    Ok(())
}

/// Events about a user's own data reach only that user's sockets, and events
/// about shared data carry each recipient's own tracked state.
#[tokio::test]
async fn broadcasts_are_per_user() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    let db = &server.state.db;
    let root = db.default_owner().await?;
    let alice = server.create_user("alice", None).await?;

    let show = api::ShowId::new(1);
    db.create_show(show, "Show", None, "").await?;
    db.set_show_tracked(root, show, true).await?;

    let mut events = server.state.broadcast.subscribe();

    server.state.broadcast.emit_to(
        alice,
        musli_web::api::ChannelId::NONE,
        api::AppEventKind::PendingChanged,
        "test",
    );
    let mine = events.recv().await?;
    assert!(mine.reaches(alice, false));
    assert!(!mine.reaches(root, true));

    let show = db.show_by_id(Some(root), show).await?.unwrap();
    assert!(show.tracked);
    server
        .state
        .broadcast
        .broadcast_event(api::AppEventKind::ShowChanged { show });
    let shared = events.recv().await?;
    assert!(shared.reaches(alice, false) && shared.reaches(root, true));

    let mut kind = shared.event.kind;
    assert!(crate::ws::personalize(db, alice, &mut kind).await?);
    let api::AppEventKind::ShowChanged { show } = &kind else {
        panic!("expected a show change");
    };
    assert!(!show.tracked, "alice does not track the show");

    crate::ws::personalize(db, root, &mut kind).await?;
    let api::AppEventKind::ShowChanged { show } = &kind else {
        panic!("expected a show change");
    };
    assert!(show.tracked, "root does");

    // Each recipient sees the show in their own language.
    let swedish = api::Locale::from_iso("sv").unwrap();
    db.set_show_language(root, show.id, swedish).await?;
    crate::ws::personalize(db, alice, &mut kind).await?;
    let api::AppEventKind::ShowChanged { show } = &kind else {
        panic!("expected a show change");
    };
    assert_eq!(show.language, api::Locale::DEFAULT);
    crate::ws::personalize(db, root, &mut kind).await?;
    let api::AppEventKind::ShowChanged { show } = &kind else {
        panic!("expected a show change");
    };
    assert_eq!(show.language, swedish);

    // An event about a show that is gone can only carry the sender's view.
    db.delete_show(show.id).await?;
    assert!(!crate::ws::personalize(db, alice, &mut kind).await?);

    // The system configuration reaches administrators only.
    server.state.broadcast.emit_to_admins(
        musli_web::api::ChannelId::NONE,
        api::AppEventKind::ConfigChanged {
            config: api::Config::default(),
        },
    );
    let config = events.recv().await?;
    assert!(config.reaches(root, true));
    assert!(!config.reaches(alice, false));
    Ok(())
}
