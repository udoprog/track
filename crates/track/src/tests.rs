//! Signing in, sessions, registration links and request authorization through
//! watchword as track wires it up, against a server on a fresh database with
//! root.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use auth::Role;
use auth::axum08::SignedIn;
use musli_web::ws::Channels;
use reqwest::StatusCode;
use reqwest::header::{CONNECTION, COOKIE, HeaderMap, HeaderValue, SET_COOKIE, UPGRADE};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::{Notify, broadcast};

use crate::app_broadcast::Broadcaster;
use crate::cache::ImageCache;
use crate::db::users::ROOT_HASH;
use crate::db::{Database, OpenMode};
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;
use crate::web::{self, AppState};
use crate::ws::{AuthUser, Refused, WsHandler};

const SESSION_COOKIE: &str = "track_session";

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
        db.test_root().await?;

        let http = reqwest::Client::new();
        let auth = crate::entry::build_auth(&db, &config, trusted_proxies).await?;
        let (tx, _) = broadcast::channel(16);

        let state = AppState {
            db: db.clone(),
            broadcast: Broadcaster::new(tx),
            channels: Channels::default(),
            cache: ImageCache::new(dir.path().join("image-cache")),
            queue: TaskQueue::new(),
            remote: RemoteClients::new(http.clone(), http),
            pending: PendingSystem::new(db),
            config_changed: Arc::new(Notify::new()),
            delay: None,
            shutdown: Shutdown::new(),
            auth,
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

    /// The signed-in user and session of a session cookie.
    async fn signed_in(&self, cookie: &str) -> Result<SignedIn> {
        let id = self
            .state
            .auth
            .session_id(&cookie_header(cookie))
            .expect("a signed session cookie");
        let (user, session) = self.state.auth.session(&id).await?.expect("a live session");
        Ok(SignedIn { user, session })
    }

    fn handler(&self, user: &SignedIn) -> WsHandler {
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
            user: Arc::new(AuthUser::from(user)),
            conn: auth::musliweb05::Connection::new(state.auth.clone(), user),
        }
    }

    /// Creates an active regular user whose password is `root`.
    async fn create_user(&self, login: &str, email: Option<&str>) -> Result<api::UserId> {
        self.state
            .db
            .test_user(login, email, Role::Regular, Some(ROOT_HASH))
            .await
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

fn cloudflare(enabled: bool, trust_email_header: bool, verify_jwt: bool) -> api::Config {
    api::Config {
        cloudflare_access: auth::CloudflareConfig {
            enabled,
            team_domain: "example.cloudflareaccess.com".to_owned(),
            audience: "aud".to_owned(),
            trust_email_header,
            verify_jwt,
        },
        ..api::Config::default()
    }
}

fn email(email: &'static str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        auth::cloudflare::EMAIL_HEADER,
        HeaderValue::from_static(email),
    );
    headers
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
    server
        .create_user("alice", Some("alice@example.com"))
        .await?;

    let cookie = server.login("Alice@Example.com", "root").await?;
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
    server
        .state
        .db
        .test_user("bob", None, Role::Regular, None)
        .await?;
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
        assert_eq!(cookies[0].name(), SESSION_COOKIE);
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

    let users = server.get("/api/auth/users", HeaderMap::new()).await?;
    assert_eq!(users.status, StatusCode::UNAUTHORIZED);

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
        .signed_in(&server.login("root", "root").await?)
        .await?;
    server.create_user("alice", None).await?;
    let alice = server.login("alice", "root").await?;
    let alice_user = server.signed_in(&alice).await?;

    let regular = server.handler(&alice_user);

    for id in [
        api::Request::GetSystemConfig,
        api::Request::SetSystemConfig,
        api::Request::RemoveShow,
        api::Request::RemoveMovie,
        api::Request::DeletePerson,
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
        api::Request::ListShowAltNames,
        api::Request::MarkWatched,
    ] {
        regular.authorize(id).await?;
    }

    let users = server.get("/api/auth/users", cookie_header(&alice)).await?;
    assert_eq!(users.status, StatusCode::FORBIDDEN);

    let admin = server.handler(&root);
    admin.authorize(api::Request::SetSystemConfig).await?;

    // A role change applies to open sockets.
    server
        .state
        .auth
        .set_role(&root.user, &root.session, alice_user.user.id, Role::Admin)
        .await?;
    regular.authorize(api::Request::SetSystemConfig).await?;
    Ok(())
}

/// An administrator adds a user, whose registration link signs them in once.
#[tokio::test]
async fn users_register_through_links() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    let root = server.login("root", "root").await?;

    let created = server
        .post(
            "/api/auth/users",
            json!({"login": "alice", "email": "alice@example.com", "role": "regular"}),
            Some(&root),
        )
        .await?;
    assert_eq!(created.status, StatusCode::OK);
    let token = created.body.unwrap()["link"]["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let path = format!("/api/auth/register/{token}");

    let info = server.get(&path, HeaderMap::new()).await?;
    assert_eq!(info.status, StatusCode::OK);
    let info = info.body.unwrap();
    assert_eq!(info["login"], "alice");
    assert_eq!(info["email"], "alice@example.com");

    let register = server
        .post(&path, json!({"password": "new password"}), None)
        .await?;
    assert_eq!(register.status, StatusCode::OK);
    let cookie = register.cookie.expect("registering signs in");
    assert_eq!(server.me(&cookie).await?.body.unwrap()["login"], "alice");

    let again = server
        .post(&path, json!({"password": "other password"}), None)
        .await?;
    assert_eq!(again.status, StatusCode::GONE);

    Ok(())
}

/// Whether the socket's session ends within half a second.
async fn ended(guard: &auth::SessionGuard) -> bool {
    tokio::time::timeout(Duration::from_millis(500), guard.ended())
        .await
        .is_ok()
}

/// Ending a session, here signing out everywhere and changing the password,
/// ends the sockets of the sessions it ends and no others.
#[tokio::test]
async fn ending_sessions_ends_their_sockets() -> Result<()> {
    let server = Server::start(api::Config::default()).await?;
    let root = server.login("root", "root").await?;
    server.create_user("alice", None).await?;

    let current = server.login("alice", "root").await?;
    let other = server.login("alice", "root").await?;
    let signed_in = server.signed_in(&current).await?;
    let current_socket = server.state.auth.guard(&signed_in.user, &signed_in.session);
    let signed_in = server.signed_in(&other).await?;
    let other_socket = server.state.auth.guard(&signed_in.user, &signed_in.session);

    let change = server
        .post(
            "/api/auth/account/password",
            json!({"old_password": "root", "new_password": "new password"}),
            Some(&current),
        )
        .await?;
    assert_eq!(change.status, StatusCode::OK);
    assert!(
        ended(&other_socket).await,
        "the other session's socket ends"
    );
    assert!(!ended(&current_socket).await, "this session's socket stays");

    let alice = server.signed_in(&current).await?.user.id;
    let sign_out = server
        .post(
            &format!("/api/auth/users/{alice}/sign-out"),
            json!({}),
            Some(&root),
        )
        .await?;
    assert_eq!(sign_out.status, StatusCode::NO_CONTENT);
    assert!(
        ended(&current_socket).await,
        "signing out everywhere ends it"
    );
    assert_eq!(server.me(&current).await?.status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn cloudflare_matches_email_only() -> Result<()> {
    let server = Server::start(cloudflare(true, true, false)).await?;
    server
        .create_user("alice", Some("alice@example.com"))
        .await?;

    let config = server
        .get("/api/auth/config", email("Alice@Example.com"))
        .await?;
    assert_eq!(config.body.unwrap()["cloudflare"], true);
    assert!(config.cookie.is_none());
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
        let config = server.get("/api/auth/config", headers.clone()).await?;
        assert_eq!(config.body.unwrap()["cloudflare"], false);
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

    // A session cookie wins over the Access header.
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
    Ok(())
}

#[tokio::test]
async fn cloudflare_rejects_disabled_and_invalid_credentials() -> Result<()> {
    for enabled in [false, true] {
        let server = Server::start(cloudflare(enabled, true, true)).await?;
        server
            .create_user("alice", Some("alice@example.com"))
            .await?;
        let mut headers = email("alice@example.com");
        headers.insert(
            auth::cloudflare::JWT_HEADER,
            HeaderValue::from_static("a.b.c"),
        );
        let config = server.get("/api/auth/config", headers.clone()).await?;
        assert_eq!(config.body.unwrap()["cloudflare"], false);
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
