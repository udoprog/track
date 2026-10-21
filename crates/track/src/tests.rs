//! track's wiring of watchword, against a server on a fresh database with
//! root: its auth routes and session cookie, trusted proxies, which requests
//! need a session or an administrator, the Cloudflare Access settings kept in
//! its database, and per-user broadcasts. watchword's own sign-in, link,
//! throttling and user-management rules are tested by its store suite, run
//! here over track's migrated database.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use anyhow::Result;
use auth::axum08::SignedIn;
use auth::{CloudflareConfig, Role};
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
use crate::entry::{SESSION_COOKIE, build_auth};
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::shutdown::Shutdown;
use crate::task_queue::TaskQueue;
use crate::web::{self, AppState};
use crate::ws::{AuthUser, Refused, WsHandler};

/// watchword's store conformance suite over track's migrated schema.
mod store {
    auth::store_suite!(crate::db::Database::open_scratch()?.auth_store());
}

/// The password of every test user.
const PASSWORD: &str = "root";

struct Server {
    url: String,
    state: AppState,
    client: reqwest::Client,
    _dir: TempDir,
}

impl Server {
    async fn start() -> Result<Self> {
        Self::start_trusting(&[]).await
    }

    async fn start_trusting(trusted_proxies: &[IpAddr]) -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let db = Database::open(dir.path().join("track.db"), OpenMode::Bulk, 1)?;
        db.test_root().await?;

        let http = reqwest::Client::new();
        let auth = build_auth(&db, trusted_proxies).await?;
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

    async fn post(&self, path: &str, body: Value, headers: HeaderMap) -> Result<Response> {
        let req = self
            .client
            .post(format!("{}{path}", self.url))
            .header("content-type", "application/json")
            .headers(headers)
            .body(serde_json::to_vec(&body)?);

        Response::from(req.send().await?).await
    }

    async fn get(&self, path: &str, headers: HeaderMap) -> Result<Response> {
        let req = self
            .client
            .get(format!("{}{path}", self.url))
            .headers(headers);
        Response::from(req.send().await?).await
    }

    /// A new session of the user `login`, made without signing in.
    async fn signed_in(&self, login: &str) -> Result<SignedIn> {
        let auth = &self.state.auth;
        let user = auth.find_user(login).await?.expect("the user exists");
        let session = auth.test_session(&user).await?;
        Ok(SignedIn { user, session })
    }

    /// The `Cookie` header of `signed_in`'s session.
    fn cookie(&self, signed_in: &SignedIn) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, self.state.auth.cookie_header(&signed_in.session.id));
        headers
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

    /// Creates an active regular user who signs in with [`PASSWORD`].
    async fn create_user(&self, login: &str, email: Option<&str>) -> Result<api::UserId> {
        self.state
            .db
            .test_user(login, email, Role::Regular, Some(PASSWORD))
            .await
    }
}

struct Response {
    status: StatusCode,
    headers: HeaderMap,
    body: Option<Value>,
}

impl Response {
    async fn from(response: reqwest::Response) -> Result<Self> {
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await?;
        let body = serde_json::from_slice(&bytes).ok();

        Ok(Self {
            status,
            headers,
            body,
        })
    }

    /// The session cookie the response sets, as a `Cookie` header.
    fn session_cookie(&self) -> HeaderMap {
        let cookie = self
            .headers
            .get_all(SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok()?.split(';').next())
            .find(|pair| pair.starts_with(&format!("{SESSION_COOKIE}=")))
            .expect("the response sets the session cookie");

        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_str(cookie).unwrap());
        headers
    }
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

/// watchword's routes are served under /api/auth with track's session cookie,
/// and sign in by login name.
#[tokio::test]
async fn signs_in_with_the_track_session_cookie() -> Result<()> {
    let server = Server::start().await?;

    let login = server
        .post(
            "/api/auth/login",
            json!({"login": "root", "password": PASSWORD}),
            HeaderMap::new(),
        )
        .await?;
    assert_eq!(login.status, StatusCode::OK);

    let me = server.get("/api/auth/me", login.session_cookie()).await?;
    assert_eq!(me.status, StatusCode::OK);
    let me = me.body.unwrap();
    assert_eq!(me["login"], "root");
    assert_eq!(me["role"], "admin");
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
    let server = Server::start().await?;
    assert_eq!(
        forwarded_attempts(&server).await?,
        StatusCode::TOO_MANY_REQUESTS
    );

    let server = Server::start_trusting(&["127.0.0.1".parse()?]).await?;
    assert_eq!(forwarded_attempts(&server).await?, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() -> Result<()> {
    let server = Server::start().await?;

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

    let root = server.signed_in("root").await?;
    let mut headers = upgrade_headers();
    headers.extend(server.cookie(&root));
    let ws = server.get("/ws", headers).await?;
    assert_eq!(ws.status, StatusCode::SWITCHING_PROTOCOLS);
    Ok(())
}

#[tokio::test]
async fn admin_only_requests() -> Result<()> {
    let server = Server::start().await?;
    let root = server.signed_in("root").await?;
    server.create_user("alice", None).await?;
    let alice_user = server.signed_in("alice").await?;

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

    let users = server
        .get("/api/auth/users", server.cookie(&alice_user))
        .await?;
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

/// Cloudflare Access settings saved by an administrator are kept in track's
/// database, sign users in by email, and apply again after a restart.
#[tokio::test]
async fn cloudflare_settings_are_kept_in_the_database() -> Result<()> {
    let server = Server::start().await?;
    server
        .create_user("alice", Some("alice@example.com"))
        .await?;

    let settings = CloudflareConfig {
        enabled: true,
        team_domain: "example.cloudflareaccess.com".to_owned(),
        audience: "aud".to_owned(),
        trust_email_header: true,
        verify_jwt: false,
    };

    let root = server.signed_in("root").await?;
    server
        .state
        .auth
        .set_cloudflare(&root.user, &root.session, settings.clone())
        .await?;

    let mut email = HeaderMap::new();
    email.insert(
        auth::cloudflare::EMAIL_HEADER,
        HeaderValue::from_static("Alice@Example.com"),
    );

    let login = server
        .post("/api/auth/cloudflare", json!({}), email)
        .await?;
    assert_eq!(login.status, StatusCode::OK);
    assert_eq!(login.body.unwrap()["login"], "alice");

    let restarted = build_auth(&server.state.db, &[]).await?;
    assert_eq!(restarted.cloudflare().await?, Some(settings));
    Ok(())
}

/// Events about a user's own data reach only that user's sockets, and events
/// about shared data carry each recipient's own tracked state.
#[tokio::test]
async fn broadcasts_are_per_user() -> Result<()> {
    let server = Server::start().await?;
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
