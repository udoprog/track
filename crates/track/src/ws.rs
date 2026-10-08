use core::iter;

use core::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Error, Result};
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use musli_web::axum08;
use musli_web::ws;
use tokio::sync::broadcast;
use tokio::time;

use crate::app_broadcast::{Audience, Broadcaster};
use crate::db::Database;
use crate::db::users::{Conflict, UserRecord};
use crate::identity::{Auth, AuthUser};
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::task_queue::TaskQueue;
use crate::web::AppState;

mod images;
mod library;
mod movies;
mod people;
mod remotes;
mod settings;
mod shows;
mod tasks;
mod users;
mod watched;

/// An artificial random delay applied to every websocket request, used to
/// preview loading/skeleton states on slow connections. Parsed from a
/// `MIN..MAX` millisecond range on the command line.
#[derive(Debug, Clone, Copy)]
pub struct RandomDelay {
    min: u64,
    max: u64,
}

impl RandomDelay {
    /// A random delay within the configured inclusive range.
    fn sample(self) -> Duration {
        Duration::from_millis(rand::random_range(self.min..=self.max))
    }
}

impl FromStr for RandomDelay {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (min, max) = s.split_once("..").context("expected a `MIN..MAX`")?;

        let min_ms = min.trim().parse().context("parsing minimum delay")?;

        let max_ms = max.trim().parse().context("parsing maximum delay")?;

        if min_ms > max_ms {
            return Err(anyhow::anyhow!(
                "MIN ({min_ms}) must not exceed MAX ({max_ms})"
            ));
        }

        Ok(Self {
            min: min_ms,
            max: max_ms,
        })
    }
}

#[derive(Clone)]
pub(super) struct WsHandler {
    pub(super) db: Database,
    pub(super) broadcast: Broadcaster,
    pub(super) remote: RemoteClients,
    pub(super) queue: TaskQueue,
    pub(super) pending: PendingSystem,
    pub(super) config_changed: Arc<tokio::sync::Notify>,
    pub(super) delay: Option<RandomDelay>,
    pub(super) auth: Auth,
    pub(super) user: Arc<AuthUser>,
}

/// A request refused for a reason the user can act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    NotAdmin,
    OwnAccount,
    NoSuchUser,
    EmptyLogin,
    LoginTaken,
    EmailTaken,
    WrongPassword,
    WeakPassword(&'static str),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refused::NotAdmin => f.write_str("Only administrators can do this."),
            Refused::OwnAccount => f.write_str("You cannot do this to your own account."),
            Refused::NoSuchUser => f.write_str("No such user."),
            Refused::EmptyLogin => f.write_str("The login cannot be empty."),
            Refused::LoginTaken => f.write_str("That login is already in use."),
            Refused::EmailTaken => f.write_str("That email is already in use."),
            Refused::WrongPassword => f.write_str("The current password is incorrect."),
            Refused::WeakPassword(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Refused {}

impl From<Conflict> for Refused {
    fn from(conflict: Conflict) -> Self {
        match conflict {
            Conflict::Login => Refused::LoginTaken,
            Conflict::Email => Refused::EmailTaken,
        }
    }
}

/// Requests only administrators may make.
fn requires_admin(id: api::Request) -> bool {
    matches!(
        id,
        api::Request::GetSystemConfig
            | api::Request::SetSystemConfig
            | api::Request::RemoveShow
            | api::Request::RemoveMovie
            | api::Request::DeletePerson
            | api::Request::ListUsers
            | api::Request::CreateUser
            | api::Request::SetUserRole
            | api::Request::DeleteUser
            | api::Request::GenerateLoginToken
            | api::Request::RevokeLoginToken
            | api::Request::RevokeUserAccess
            | api::Request::AddShowRemote
            | api::Request::RemoveShowRemote
            | api::Request::UpdateShowRemote
            | api::Request::SetShowRemoteEnabled
            | api::Request::SetShowRemoteSyncKinds
            | api::Request::ReorderShowRemotes
            | api::Request::PurgeShowRemoteCache
            | api::Request::PurgeEpisodeCache
            | api::Request::SetShowAutoSync
            | api::Request::SetShowAirDateFilters
            | api::Request::SetShowNumbering
            | api::Request::AddMovieRemote
            | api::Request::RemoveMovieRemote
            | api::Request::UpdateMovieRemote
            | api::Request::SetMovieRemoteEnabled
            | api::Request::SetMovieRemoteSyncKinds
            | api::Request::ReorderMovieRemotes
            | api::Request::PurgeMovieRemoteCache
            | api::Request::SetMovieAutoSync
            | api::Request::SetMovieReleaseFilters
            | api::Request::AddPersonRemote
            | api::Request::RemovePersonRemote
            | api::Request::UpdatePersonRemote
            | api::Request::SetPersonRemoteEnabled
            | api::Request::SetPersonRemoteSyncKinds
            | api::Request::ReorderPersonRemotes
            | api::Request::PurgePersonRemoteCache
            | api::Request::SelectImage
            | api::Request::ClearSelectedImage
            | api::Request::PickBestImages
            | api::Request::ResetImageSelection
            | api::Request::SyncAll
            | api::Request::RemoveTask
            | api::Request::BumpTask
    )
}

fn parse_login(login: &str) -> Result<String, Refused> {
    let login = login.trim();

    if login.is_empty() {
        return Err(Refused::EmptyLogin);
    }

    Ok(login.to_owned())
}

/// A normalized email, with a blank one meaning none.
fn parse_email(email: Option<&str>) -> Option<String> {
    email.map(auth::normalize_email).filter(|e| !e.is_empty())
}

fn role_from_api(role: api::UserRole) -> auth::UserRole {
    match role {
        api::UserRole::Admin => auth::UserRole::Admin,
        api::UserRole::Regular => auth::UserRole::Regular,
    }
}

impl ws::Handler for WsHandler {
    type Id = api::Request;
    type Response = Result<()>;

    async fn handle(
        &self,
        id: Self::Id,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Self::Response {
        tracing::trace!(?id, "Request");

        // Optional artificial latency so loading/skeleton states can be observed
        // against a slow connection.
        if let Some(delay) = self.delay {
            time::sleep(delay.sample()).await;
        }

        let result = self.handle_inner(id, incoming, outgoing).await;

        if let Err(error) = &result {
            tracing::error!(?error);

            for cause in error.chain().skip(1) {
                tracing::error!(?cause);
            }
        }

        result
    }
}

impl WsHandler {
    async fn enqueue_show_sync(
        &self,
        show_id: api::ShowId,
        title: Option<String>,
        immediate: bool,
    ) {
        self.queue
            .push(
                api::TaskKind::SyncShow { show_id, title },
                immediate,
                &self.broadcast,
            )
            .await;
    }

    async fn enqueue_movie_sync(
        &self,
        movie_id: api::MovieId,
        title: Option<String>,
        immediate: bool,
    ) {
        self.queue
            .push(
                api::TaskKind::SyncMovie { movie_id, title },
                immediate,
                &self.broadcast,
            )
            .await;
    }

    async fn enqueue_episode_sync(
        &self,
        show_id: api::ShowId,
        episode_id: api::EpisodeId,
        code: api::Code,
        title: Option<String>,
        immediate: bool,
    ) {
        self.queue
            .push(
                api::TaskKind::SyncEpisode {
                    show_id,
                    episode_id,
                    code,
                    title,
                },
                immediate,
                &self.broadcast,
            )
            .await;
    }

    async fn enqueue_person_sync(
        &self,
        person_id: api::PersonId,
        title: Option<String>,
        immediate: bool,
    ) {
        self.queue
            .push(
                api::TaskKind::SyncPerson { person_id, title },
                immediate,
                &self.broadcast,
            )
            .await;
    }

    /// Mark a watch, advance pending past it and tell the user's other sockets.
    async fn mark_watched(
        &self,
        channel: musli_web::api::ChannelId,
        kind: api::WatchedKind,
        mark_time: api::MarkTime,
        now: api::Timestamp,
    ) -> Result<api::MarkWatchedResponse> {
        let pending_before = self.db.pending_before(self.user.id, kind).await?;

        let watched = self
            .db
            .mark_watched(self.user.id, api::WatchedId::random(), kind, mark_time, now)
            .await?;

        match kind {
            api::WatchedKind::Episode { show, episode } => {
                self.pending
                    .on_episode_watched_from(self.user.id, show, episode, now)
                    .await?;
            }
            api::WatchedKind::Movie { movie } => {
                self.db.remove_pending_movie(self.user.id, movie).await?;
            }
        }

        self.broadcast.emit_to(
            self.user.id,
            channel,
            api::AppEventKind::WatchedChanged {
                event: kind.into_event(),
            },
            "ws mark watched changed",
        );

        self.broadcast.emit_to(
            self.user.id,
            channel,
            api::AppEventKind::PendingChanged,
            "ws mark watched pending changed",
        );

        Ok(api::MarkWatchedResponse {
            watched,
            pending_before,
        })
    }

    /// The cutoff pending items are listed up to: now shifted forward by the
    /// configured dashboard lookahead, so items surface before they air.
    async fn pending_cutoff(&self) -> Result<api::Timestamp> {
        let preferences = self.db.load_preferences(self.user.id).await?;
        Ok(api::Timestamp::now().saturating_add(preferences.dashboard_lookahead))
    }

    /// Fill the current user's pending slot for a show they started tracking.
    async fn fill_my_pending(&self, show: &api::Show) -> Result<()> {
        self.db
            .fill_pending_for_user_show(self.user.id, show.id, api::Timestamp::now())
            .await
    }

    /// The current user as stored now, so role changes apply to open sockets.
    async fn current_user(&self) -> Result<UserRecord> {
        let user = self.db.user_by_id(self.user.id).await?;
        Ok(user.ok_or(Refused::NoSuchUser)?)
    }

    /// Refuses admin-only requests from anyone else.
    pub(crate) async fn authorize(&self, id: api::Request) -> Result<()> {
        if requires_admin(id) && self.current_user().await?.role != auth::UserRole::Admin {
            return Err(Refused::NotAdmin.into());
        }

        Ok(())
    }

    /// Refuses to act on the current user's own account.
    fn not_self(&self, user_id: api::UserId) -> Result<(), Refused> {
        if user_id == self.user.id {
            return Err(Refused::OwnAccount);
        }

        Ok(())
    }

    async fn handle_inner(
        &self,
        id: api::Request,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        self.authorize(id).await?;

        match id {
            api::Request::ListMedia => self.list_media(incoming, outgoing).await?,
            api::Request::GetShow => self.get_show(incoming, outgoing).await?,
            api::Request::GetTranslations => self.get_translations(incoming, outgoing).await?,
            api::Request::ListSeasons => self.list_seasons(incoming, outgoing).await?,
            api::Request::ListCredits => self.list_credits(incoming, outgoing).await?,
            api::Request::ListPersons => self.list_persons(incoming, outgoing).await?,
            api::Request::GetPerson => self.get_person(incoming, outgoing).await?,
            api::Request::ListPersonCredits => self.list_person_credits(incoming, outgoing).await?,
            api::Request::AddPersonRemote => self.add_person_remote(incoming, outgoing).await?,
            api::Request::RemovePersonRemote => {
                self.remove_person_remote(incoming, outgoing).await?
            }
            api::Request::UpdatePersonRemote => {
                self.update_person_remote(incoming, outgoing).await?
            }
            api::Request::SetPersonRemoteEnabled => {
                self.set_person_remote_enabled(incoming, outgoing).await?
            }
            api::Request::ReorderPersonRemotes => {
                self.reorder_person_remotes(incoming, outgoing).await?
            }
            api::Request::SetPersonRemoteSyncKinds => {
                self.set_person_remote_sync_kinds(incoming, outgoing)
                    .await?
            }
            api::Request::PurgePersonRemoteCache => {
                self.purge_person_remote_cache(incoming, outgoing).await?
            }
            api::Request::DeletePerson => self.delete_person(incoming, outgoing).await?,
            api::Request::GetSeasonImages => self.get_season_images(incoming, outgoing).await?,
            api::Request::TrackShow => self.track_show(incoming, outgoing).await?,
            api::Request::UntrackShow => self.untrack_show(incoming, outgoing).await?,
            api::Request::RemoveShow => self.remove_show(incoming, outgoing).await?,
            api::Request::ListEpisodes => self.list_episodes(incoming, outgoing).await?,
            api::Request::FindEpisodeByTimestamp => {
                self.find_episode_by_timestamp(incoming, outgoing).await?
            }
            api::Request::GetEpisodeReleases => {
                self.get_episode_releases(incoming, outgoing).await?
            }
            api::Request::GetEpisodeCache => self.get_episode_cache(incoming, outgoing).await?,
            api::Request::PurgeEpisodeCache => self.purge_episode_cache(incoming, outgoing).await?,
            api::Request::GetMovieReleases => self.get_movie_releases(incoming, outgoing).await?,
            api::Request::GetMovie => self.get_movie(incoming, outgoing).await?,
            api::Request::TrackMovie => self.track_movie(incoming, outgoing).await?,
            api::Request::UntrackMovie => self.untrack_movie(incoming, outgoing).await?,
            api::Request::RemoveMovie => self.remove_movie(incoming, outgoing).await?,
            api::Request::MarkWatched => self.mark_watched_request(incoming, outgoing).await?,
            api::Request::MarkNextEpisode => self.mark_next_episode(incoming, outgoing).await?,
            api::Request::MarkWatchedRemaining => {
                self.mark_watched_remaining(incoming, outgoing).await?
            }
            api::Request::RemoveWatched => self.remove_watched(incoming, outgoing).await?,
            api::Request::UndoWatched => self.undo_watched(incoming, outgoing).await?,
            api::Request::ListEpisodesWatched => {
                self.list_episodes_watched(incoming, outgoing).await?
            }
            api::Request::ListWatched => self.list_watched(incoming, outgoing).await?,
            api::Request::MoveWatchedEpisode => {
                self.move_watched_episode(incoming, outgoing).await?
            }
            api::Request::ListOrphanedWatched => {
                self.list_orphaned_watched(incoming, outgoing).await?
            }
            api::Request::ListPending => self.list_pending(incoming, outgoing).await?,
            api::Request::ListSchedule => self.list_schedule(incoming, outgoing).await?,
            api::Request::ListWatchNext => self.list_watch_next(incoming, outgoing).await?,
            api::Request::Search => self.search(incoming, outgoing).await?,
            api::Request::SyncShow => self.sync_show(incoming, outgoing).await?,
            api::Request::SyncEpisode => self.sync_episode(incoming, outgoing).await?,
            api::Request::SyncMovie => self.sync_movie(incoming, outgoing).await?,
            api::Request::SyncPerson => self.sync_person(incoming, outgoing).await?,
            api::Request::SetShowRemoteEnabled => {
                self.set_show_remote_enabled(incoming, outgoing).await?
            }
            api::Request::ReorderShowRemotes => {
                self.reorder_show_remotes(incoming, outgoing).await?
            }
            api::Request::SetMovieRemoteEnabled => {
                self.set_movie_remote_enabled(incoming, outgoing).await?
            }
            api::Request::ReorderMovieRemotes => {
                self.reorder_movie_remotes(incoming, outgoing).await?
            }
            api::Request::SetShowRemoteSyncKinds => {
                self.set_show_remote_sync_kinds(incoming, outgoing).await?
            }
            api::Request::SetMovieRemoteSyncKinds => {
                self.set_movie_remote_sync_kinds(incoming, outgoing).await?
            }
            api::Request::AddShowRemote => self.add_show_remote(incoming, outgoing).await?,
            api::Request::RemoveShowRemote => self.remove_show_remote(incoming, outgoing).await?,
            api::Request::AddMovieRemote => self.add_movie_remote(incoming, outgoing).await?,
            api::Request::RemoveMovieRemote => self.remove_movie_remote(incoming, outgoing).await?,
            api::Request::PurgeShowRemoteCache => {
                self.purge_show_remote_cache(incoming, outgoing).await?
            }
            api::Request::PurgeMovieRemoteCache => {
                self.purge_movie_remote_cache(incoming, outgoing).await?
            }
            api::Request::UpdateShowRemote => self.update_show_remote(incoming, outgoing).await?,
            api::Request::UpdateMovieRemote => self.update_movie_remote(incoming, outgoing).await?,
            api::Request::SetShowLanguage => self.set_show_language(incoming, outgoing).await?,
            api::Request::SetShowIncludeSpecials => {
                self.set_show_include_specials(incoming, outgoing).await?
            }
            api::Request::SetShowAutoSync => self.set_show_auto_sync(incoming, outgoing).await?,
            api::Request::SetMovieAutoSync => self.set_movie_auto_sync(incoming, outgoing).await?,
            api::Request::SetShowAirDateFilters => {
                self.set_show_air_date_filters(incoming, outgoing).await?
            }
            api::Request::SetShowNumbering => self.set_show_numbering(incoming, outgoing).await?,
            api::Request::GetShowNumbering => self.get_show_numbering(incoming, outgoing).await?,
            api::Request::SetMovieLanguage => self.set_movie_language(incoming, outgoing).await?,
            api::Request::SetMovieReleaseFilters => {
                self.set_movie_release_filters(incoming, outgoing).await?
            }
            api::Request::SyncAll => self.sync_all(incoming, outgoing).await?,
            api::Request::ListTasks => self.list_tasks(incoming, outgoing).await?,
            api::Request::RemoveTask => self.remove_task(incoming, outgoing).await?,
            api::Request::BumpTask => self.bump_task(incoming, outgoing).await?,
            api::Request::GetSystemConfig => self.get_system_config(incoming, outgoing).await?,
            api::Request::GetPreferences => self.get_preferences(incoming, outgoing).await?,
            api::Request::SetPreferences => self.set_preferences(incoming, outgoing).await?,
            api::Request::GetTopLanguages => self.get_top_languages(incoming, outgoing).await?,
            api::Request::SetSystemConfig => self.set_system_config(incoming, outgoing).await?,
            api::Request::AddPending => self.add_pending(incoming, outgoing).await?,
            api::Request::RemovePending => self.remove_pending(incoming, outgoing).await?,
            api::Request::SkipEpisode => self.skip_episode(incoming, outgoing).await?,
            api::Request::SelectImage => self.select_image(incoming, outgoing).await?,
            api::Request::ClearSelectedImage => {
                self.clear_selected_image(incoming, outgoing).await?
            }
            api::Request::PickBestImages => self.pick_best_images(incoming, outgoing).await?,
            api::Request::ResetImageSelection => {
                self.reset_image_selection(incoming, outgoing).await?
            }
            api::Request::ListUsers => self.list_users(incoming, outgoing).await?,
            api::Request::CreateUser => self.create_user(incoming, outgoing).await?,
            api::Request::SetUserRole => self.set_user_role(incoming, outgoing).await?,
            api::Request::DeleteUser => self.delete_user(incoming, outgoing).await?,
            api::Request::GenerateLoginToken => {
                self.generate_login_token(incoming, outgoing).await?
            }
            api::Request::RevokeLoginToken => self.revoke_login_token(incoming, outgoing).await?,
            api::Request::RevokeUserAccess => self.revoke_user_access(incoming, outgoing).await?,
            api::Request::SetLogin => self.set_login(incoming, outgoing).await?,
            api::Request::SetEmail => self.set_email(incoming, outgoing).await?,
            api::Request::SetPassword => self.set_password(incoming, outgoing).await?,
            api::Request::Unknown(id) => {
                anyhow::bail!("Unknown request id: {id:?}");
            }
        }

        Ok(())
    }
}

/// Whether the user is an administrator now; a lookup failure counts as not.
async fn is_admin(db: &Database, user: api::UserId) -> bool {
    match db.user_by_id(user).await {
        Ok(user) => user.is_some_and(|u| u.role == auth::UserRole::Admin),
        Err(error) => {
            tracing::error!("Looking up user {user}: {error:#}");
            false
        }
    }
}

/// Rewrites the per-user parts of an event about shared data (tracked, pending,
/// watched counts) for the user receiving it.
///
/// Returns whether the event may be sent: one whose entity is gone can only
/// carry the sender's view of it, so it is not.
pub(crate) async fn personalize(
    db: &Database,
    user: api::UserId,
    kind: &mut api::AppEventKind,
) -> Result<bool> {
    match kind {
        api::AppEventKind::ShowCreated { show } | api::AppEventKind::ShowChanged { show } => {
            let Some(mine) = db.show_by_id(Some(user), show.id).await? else {
                return Ok(false);
            };

            *show = mine;
        }
        api::AppEventKind::MovieCreated { movie } | api::AppEventKind::MovieChanged { movie } => {
            let Some(mine) = db.movie_by_id(Some(user), movie.id).await? else {
                return Ok(false);
            };

            *movie = mine;
        }
        api::AppEventKind::EpisodeChanged { episode } => {
            let Some(mine) = db.episode_by_id(Some(user), episode.id).await? else {
                return Ok(false);
            };

            *episode = mine;
        }
        api::AppEventKind::SeasonsChanged { show_id, seasons } if !seasons.is_empty() => {
            *seasons = db.seasons(Some(user), *show_id).await?;
        }
        _ => {}
    }

    Ok(true)
}

fn resync() -> api::AppEvent {
    api::AppEvent {
        channel: musli_web::api::ChannelId::NONE,
        kind: api::AppEventKind::Resync,
    }
}

/// How often an open socket checks that its session is still valid.
const SESSION_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Whether the socket's session still exists, is unexpired and belongs to its
/// user. A lookup failure counts as not.
pub(crate) async fn session_valid(db: &Database, user: &AuthUser) -> bool {
    let Some(session) = &user.session else {
        return true;
    };

    match db.session_user(session, api::Timestamp::now()).await {
        Ok(found) => found.is_some_and(|u| u.id == user.id),
        Err(error) => {
            tracing::error!("Checking session: {error:#}");
            false
        }
    }
}

/// Upgrades only signed-in users; others get 401 before the upgrade.
pub(super) async fn ws_handler(
    State(state): State<AppState>,
    user: AuthUser,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| async move {
        let user = Arc::new(user);

        let handler = WsHandler {
            db: state.db.clone(),
            broadcast: state.broadcast.clone(),
            remote: state.remote.clone(),
            queue: state.queue.clone(),
            pending: state.pending.clone(),
            config_changed: state.config_changed.clone(),
            delay: state.delay,
            auth: state.auth.clone(),
            user: user.clone(),
        };

        let mut subscribe = state.broadcast.subscribe();
        let mut revocations = state.auth.subscribe_revocations();
        let mut session_check = time::interval_at(
            time::Instant::now() + SESSION_CHECK_INTERVAL,
            SESSION_CHECK_INTERVAL,
        );

        let connect =
            axum08::server(socket, handler).with_channel_allocator(state.channels.clone());

        let mut server = match connect.connect().await {
            Ok(server) => server,
            Err(error) => {
                tracing::error!("WebSocket negotiation failed: {error}");
                return;
            }
        };

        loop {
            tokio::select! {
                m = subscribe.recv() => {
                    let msg = match m {
                        Ok(msg) => msg,
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "Socket fell behind on broadcasts, resyncing it");
                            // The client reloads everything, so the backlog is moot.
                            subscribe = subscribe.resubscribe();

                            if let Err(error) = server.broadcast(resync()) {
                                tracing::error!("Broadcast Error: {error}");
                                break;
                            }

                            continue;
                        }
                        Err(_) => break,
                    };

                    let admin = msg.audience == Audience::Admins
                        && is_admin(&state.db, user.id).await;

                    if !msg.reaches(user.id, admin) {
                        continue;
                    }

                    let mut event = msg.event;

                    // An event left unpersonalized may carry another user's
                    // state, so it is never sent.
                    match personalize(&state.db, user.id, &mut event.kind).await {
                        Ok(true) => {}
                        Ok(false) => continue,
                        Err(error) => {
                            tracing::error!("Personalizing broadcast, resyncing instead: {error:#}");
                            event = resync();
                        }
                    }

                    if let Err(error) = server.broadcast(event) {
                        tracing::error!("Broadcast Error: {error}");
                        break;
                    }
                }
                result = server.run() => {
                    if let Err(error) = result {
                        tracing::error!("WebSocket Error: {error:?}");
                        for cause in iter::successors(Some(&error as &dyn std::error::Error), |e| e.source()).skip(1) {
                            tracing::error!("Caused by: {cause}");
                        }
                    }
                    break;
                }
                revoke = revocations.recv() => {
                    match revoke {
                        Ok(revoke) if revoke.applies_to(&user) => break,
                        Ok(_) => continue,
                        // A dropped revocation may have been for this socket.
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            if !session_valid(&state.db, &user).await {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
                _ = session_check.tick() => {
                    if !session_valid(&state.db, &user).await {
                        break;
                    }
                }
                // Upgraded connections outlive the server's graceful shutdown.
                _ = state.shutdown.cancelled() => break,
            }
        }
    })
}
