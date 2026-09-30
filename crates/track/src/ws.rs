use core::iter;

use core::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Error, Result};
use api::{MovieId, ShowId, TimeZone};
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use musli_web::axum08;
use musli_web::ws;
use tokio::sync::broadcast;
use tokio::time;

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::task_queue::TaskQueue;
use crate::web::AppState;

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

    /// Broadcast that a person changed, after a mutation that does not warrant a
    /// resync (add/remove/update remote).
    async fn broadcast_person_changed(
        &self,
        channel: musli_web::api::ChannelId,
        person_id: api::PersonId,
        reason: &'static str,
    ) -> Result<()> {
        self.db
            .person_by_id(person_id)
            .await?
            .context("Expected person to exist")?;

        self.broadcast.emit(
            channel,
            api::AppEventKind::PersonChanged { person_id },
            reason,
        );

        Ok(())
    }

    /// Broadcast a person change and force a fresh sync (enable/reorder/sync-kinds/
    /// purge), mirroring the show/movie remote handlers.
    async fn resync_person(
        &self,
        channel: musli_web::api::ChannelId,
        person_id: api::PersonId,
        reason: &'static str,
    ) -> Result<()> {
        let person = self
            .db
            .person_by_id(person_id)
            .await?
            .context("Expected person to exist")?;

        self.broadcast.emit(
            channel,
            api::AppEventKind::PersonChanged { person_id },
            reason,
        );

        self.enqueue_person_sync(person_id, person.name.title().map(str::to_owned), true)
            .await;

        Ok(())
    }

    /// The cutoff pending items are listed up to: now shifted forward by the
    /// configured dashboard lookahead, so items surface before they air.
    async fn pending_cutoff(&self) -> Result<api::Timestamp> {
        let config = self.db.load_config().await?;
        Ok(api::Timestamp::now().saturating_add(config.dashboard_lookahead))
    }

    async fn handle_inner(
        &self,
        id: api::Request,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        match id {
            api::Request::ListMedia => {
                incoming
                    .read::<api::ListMediaRequest>()
                    .context("Expected a request payload")?;
                let items = self.db.media_items().await.context("Loading media")?;
                outgoing.write(api::ListMediaResponse { items });
            }
            api::Request::GetShow => {
                let req = incoming
                    .read::<api::GetShowRequest>()
                    .context("Expected a request payload")?;

                let show = self.db.show_by_id(req.id).await?;

                outgoing.write(show);
            }
            api::Request::GetTranslations => {
                let req = incoming
                    .read::<api::GetTranslationsRequest>()
                    .context("Expected a request payload")?;

                let translations = match req.target {
                    api::TranslationTarget::Show(id) => self.db.show_translations(id).await?,
                    api::TranslationTarget::Season(id) => self.db.season_translations(id).await?,
                    api::TranslationTarget::Episode(id) => self.db.episode_translations(id).await?,
                    api::TranslationTarget::Movie(id) => self.db.movie_translations(id).await?,
                };

                outgoing.write(api::GetTranslationsResponse { translations });
            }
            api::Request::ListSeasons => {
                let req = incoming
                    .read::<api::ListSeasonsRequest>()
                    .context("Expected a request payload")?;
                let seasons = self.db.seasons(req.show_id).await?;
                outgoing.write(api::ListSeasonsResponse { seasons });
            }
            api::Request::ListCredits => {
                let req = incoming
                    .read::<api::ListCreditsRequest>()
                    .context("Expected a request payload")?;

                let credits = match req.owner {
                    api::CreditOwner::Show(id) => self.db.list_show_credits(id).await?,
                    api::CreditOwner::Movie(id) => self.db.list_movie_credits(id).await?,
                };

                outgoing.write(api::ListCreditsResponse { credits });
            }
            api::Request::ListPersons => {
                incoming
                    .read::<api::ListPersonsRequest>()
                    .context("Expected a request payload")?;
                let persons = self.db.list_persons().await?;
                outgoing.write(api::ListPersonsResponse { persons });
            }
            api::Request::GetPerson => {
                let req = incoming
                    .read::<api::GetPersonRequest>()
                    .context("Expected a request payload")?;
                let person = self.db.person_by_id(req.id).await?;
                outgoing.write(person);
            }
            api::Request::ListPersonCredits => {
                let req = incoming
                    .read::<api::ListPersonCreditsRequest>()
                    .context("Expected a request payload")?;
                let credits = self.db.list_person_credits(req.id).await?;
                outgoing.write(api::ListPersonCreditsResponse { credits });
            }
            api::Request::AddPersonRemote => {
                let req = incoming
                    .read::<api::AddPersonRemoteRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .add_person_remote(req.id, req.slug.as_deref(), &req.remote)
                    .await?;

                self.broadcast_person_changed(incoming.channel(), req.id, "ws add person remote")
                    .await?;

                outgoing.write(api::Empty);
            }
            api::Request::RemovePersonRemote => {
                let req = incoming
                    .read::<api::RemovePersonRemoteRequest>()
                    .context("Expected a request payload")?;

                self.db.remove_person_remote(req.remote_id).await?;

                self.broadcast_person_changed(
                    incoming.channel(),
                    req.id,
                    "ws remove person remote",
                )
                .await?;

                outgoing.write(api::Empty);
            }
            api::Request::UpdatePersonRemote => {
                let req = incoming
                    .read::<api::UpdatePersonRemoteRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .update_person_remote(req.remote_id, req.slug.as_deref(), &req.remote)
                    .await?;

                self.broadcast_person_changed(
                    incoming.channel(),
                    req.id,
                    "ws update person remote",
                )
                .await?;

                outgoing.write(api::Empty);
            }
            api::Request::SetPersonRemoteEnabled => {
                let req = incoming
                    .read::<api::SetPersonRemoteEnabledRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_person_remote_enabled(req.remote_id, req.enabled)
                    .await?;

                self.resync_person(incoming.channel(), req.id, "ws set person remote enabled")
                    .await?;

                outgoing.write(api::Empty);
            }
            api::Request::ReorderPersonRemotes => {
                let req = incoming
                    .read::<api::ReorderPersonRemotesRequest>()
                    .context("Expected a request payload")?;

                self.db.reorder_person_remotes(req.remote_ids).await?;

                self.resync_person(incoming.channel(), req.id, "ws reorder person remotes")
                    .await?;

                outgoing.write(api::Empty);
            }
            api::Request::SetPersonRemoteSyncKinds => {
                let req = incoming
                    .read::<api::SetPersonRemoteSyncKindsRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_person_remote_sync_kinds(req.remote_id, req.sync_kinds)
                    .await?;

                self.resync_person(
                    incoming.channel(),
                    req.id,
                    "ws set person remote sync kinds",
                )
                .await?;

                outgoing.write(api::Empty);
            }
            api::Request::PurgePersonRemoteCache => {
                let req = incoming
                    .read::<api::PurgePersonRemoteCacheRequest>()
                    .context("Expected a request payload")?;

                self.db.set_person_remote_cache(req.remote_id, None).await?;

                self.resync_person(incoming.channel(), req.id, "ws purge person remote cache")
                    .await?;

                outgoing.write(api::Empty);
            }
            api::Request::DeletePerson => {
                let req = incoming
                    .read::<api::DeletePersonRequest>()
                    .context("Expected a request payload")?;

                self.db.delete_person(req.id).await?;

                // The person is gone; a PersonChanged lets open detail pages resolve
                // to "missing" and the people list drop it.
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PersonChanged { person_id: req.id },
                    "ws delete person",
                );

                outgoing.write(api::Empty);
            }
            api::Request::GetSeasonImages => {
                let req = incoming
                    .read::<api::GetSeasonImagesRequest>()
                    .context("Expected a request payload")?;
                let images = self.db.season_images_by_id(req.season_id).await?;
                outgoing.write(api::GetSeasonImagesResponse { images });
            }
            api::Request::TrackShow => {
                let req = incoming
                    .read::<api::TrackShowRequest>()
                    .context("Expected a request payload")?;

                let show_id = match self.db.show_id_by_remote(&req.remote).await? {
                    Some(id) => id,
                    None => ShowId::random(),
                };

                self.db
                    .create_show(show_id, &req.remote.value().to_string(), None, "")
                    .await?;

                self.db
                    .add_show_remote(show_id, req.slug.as_deref(), &req.remote)
                    .await?;

                let show = self
                    .db
                    .show_by_id(show_id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowCreated { show: show.clone() },
                    "ws track show created",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws track show pending changed",
                );

                self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(show);
            }
            api::Request::UntrackShow => {
                let req = incoming
                    .read::<api::UntrackShowRequest>()
                    .context("Expected a request payload")?;
                self.db.set_show_tracked(req.id, req.tracked).await?;
                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws untrack show changed",
                );
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws untrack show pending changed",
                );
                outgoing.write(api::Empty);
            }
            api::Request::RemoveShow => {
                let req = incoming
                    .read::<api::RemoveShowRequest>()
                    .context("Expected a request payload")?;
                self.db.delete_show(req.id).await?;
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowDeleted { show_id: req.id },
                    "ws remove show deleted",
                );
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws remove show pending changed",
                );
                outgoing.write(api::Empty);
            }
            api::Request::ListEpisodes => {
                let req = incoming
                    .read::<api::ListEpisodesRequest>()
                    .context("Expected a request payload")?;
                let episodes = self.db.episodes(req.show_id, req.season).await?;
                let watched = self.db.episodes_watched(req.show_id).await?;
                outgoing.write(api::ListEpisodesResponse { episodes, watched });
            }
            api::Request::FindEpisodeByTimestamp => {
                let req = incoming
                    .read::<api::FindEpisodeByTimestampRequest>()
                    .context("Expected a request payload")?;
                let matched = self
                    .db
                    .find_episode_by_timestamp(req.show_id, req.timestamp)
                    .await?;
                outgoing.write(api::FindEpisodeByTimestampResponse { matched });
            }
            api::Request::GetEpisodeReleases => {
                let req = incoming
                    .read::<api::GetEpisodeReleasesRequest>()
                    .context("Expected a request payload")?;
                let (releases, show_id, filters) =
                    self.db.episode_release_rows(req.episode_id).await?;
                outgoing.write(api::GetEpisodeReleasesResponse {
                    releases,
                    show_id,
                    filters,
                });
            }
            api::Request::GetEpisodeCache => {
                let req = incoming
                    .read::<api::GetEpisodeCacheRequest>()
                    .context("Expected a request payload")?;

                let cache = self.db.episode_cache(req.episode_id).await?;

                let mut entries = cache
                    .into_iter()
                    .map(|(source, cache)| api::EpisodeCacheEntry { source, cache })
                    .collect::<Vec<_>>();

                entries.sort_by_key(|e| e.source.as_id());
                outgoing.write(api::GetEpisodeCacheResponse { entries });
            }
            api::Request::PurgeEpisodeCache => {
                let req = incoming
                    .read::<api::PurgeEpisodeCacheRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_episode_cache(req.episode_id, req.source, None)
                    .await?;

                outgoing.write(api::Empty);
            }
            api::Request::GetMovieReleases => {
                let req = incoming
                    .read::<api::GetMovieReleasesRequest>()
                    .context("Expected a request payload")?;
                let (releases, filters) = self.db.movie_release_rows(req.movie_id).await?;
                outgoing.write(api::GetMovieReleasesResponse { releases, filters });
            }
            api::Request::GetMovie => {
                let req = incoming
                    .read::<api::GetMovieRequest>()
                    .context("Expected a request payload")?;
                let movie = self.db.movie_by_id(req.id).await?;
                outgoing.write(movie);
            }
            api::Request::TrackMovie => {
                let req = incoming
                    .read::<api::TrackMovieRequest>()
                    .context("Expected a request payload")?;

                let movie_id = match self.db.movie_id_by_remote(&req.remote).await? {
                    Some(id) => id,
                    None => MovieId::random(),
                };

                self.db
                    .create_movie(movie_id, &req.remote.value().to_string(), None, "", true)
                    .await?;

                self.db
                    .add_movie_remote(movie_id, req.slug.as_deref(), &req.remote)
                    .await?;

                let movie = self
                    .db
                    .movie_by_id(movie_id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieCreated {
                        movie: movie.clone(),
                    },
                    "ws track movie created",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws track movie pending changed",
                );

                self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(movie);
            }
            api::Request::UntrackMovie => {
                let req = incoming
                    .read::<api::UntrackMovieRequest>()
                    .context("Expected a request payload")?;

                self.db.set_movie_tracked(req.id, req.tracked).await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws untrack movie changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws untrack movie pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::RemoveMovie => {
                let req = incoming
                    .read::<api::RemoveMovieRequest>()
                    .context("Expected a request payload")?;

                self.db.delete_movie(req.id).await?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieDeleted { movie_id: req.id },
                    "ws remove movie deleted",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws remove movie pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::MarkWatched => {
                let req = incoming
                    .read::<api::MarkWatchedRequest>()
                    .context("Expected a request payload")?;

                let now = api::Timestamp::now();
                let pending_before = self.db.pending_before(req.kind).await?;

                let watched = self
                    .db
                    .mark_watched(api::WatchedId::random(), req.kind, req.mark_time, now)
                    .await?;

                match req.kind {
                    api::WatchedKind::Episode { show, episode } => {
                        self.pending
                            .on_episode_watched_from(show, episode, now)
                            .await?;
                    }
                    api::WatchedKind::Movie { movie } => {
                        self.db.remove_pending_movie(movie).await?;
                    }
                }

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::WatchedChanged {
                        event: req.kind.into_event(),
                    },
                    "ws mark watched changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws mark watched pending changed",
                );

                outgoing.write(api::MarkWatchedResponse {
                    watched,
                    pending_before,
                });
            }
            api::Request::MarkWatchedRemaining => {
                let req = incoming
                    .read::<api::MarkWatchedRemainingRequest>()
                    .context("Expected a request payload")?;

                let now = api::Timestamp::now();

                self.db
                    .mark_watched_remaining(req.show_id, req.season, req.mark_time, now)
                    .await?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::WatchedChanged {
                        event: api::WatchedEvent::RemainingSeason {
                            show: req.show_id,
                            season: req.season,
                        },
                    },
                    "ws mark watched changed remaining season",
                );

                outgoing.write(api::Empty);
            }
            api::Request::RemoveWatched => {
                let req = incoming
                    .read::<api::RemoveWatchedRequest>()
                    .context("Expected a request payload")?;

                self.db.remove_watched(req.id).await?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::WatchedChanged {
                        event: req.kind.into_event(),
                    },
                    "ws remove watched changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws remove watched pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::UndoWatched => {
                let req = incoming
                    .read::<api::UndoWatchedRequest>()
                    .context("Expected a request payload")?;

                self.db.remove_watched(req.id).await?;

                match (req.kind, req.pending_before) {
                    (
                        api::WatchedKind::Episode { show, .. },
                        api::PendingBefore::Episode { episode, timestamp },
                    ) => {
                        self.db
                            .add_pending_episode(show, episode, timestamp)
                            .await?;
                    }
                    (api::WatchedKind::Episode { show, .. }, _) => {
                        self.db.remove_pending_episode(show).await?;
                    }
                    (
                        api::WatchedKind::Movie { movie },
                        api::PendingBefore::Movie { timestamp },
                    ) => {
                        self.db.add_pending_movie(movie, timestamp).await?;
                    }
                    (api::WatchedKind::Movie { .. }, _) => {}
                }

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::WatchedChanged {
                        event: req.kind.into_event(),
                    },
                    "ws undo watched changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws undo watched pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::ListEpisodesWatched => {
                let req = incoming
                    .read::<api::ListEpisodesWatchedRequest>()
                    .context("Expected a request payload")?;

                let watched = self.db.episodes_watched(req.show_id).await?;
                outgoing.write(api::ListEpisodesWatchedResponse { watched });
            }
            api::Request::ListWatched => {
                let req = incoming
                    .read::<api::ListWatchedRequest>()
                    .context("Expected a request payload")?;

                let watched = match req.kind {
                    api::WatchedKind::Episode { episode, .. } => {
                        self.db.watched_for_episode(episode).await?
                    }
                    api::WatchedKind::Movie { movie } => self.db.watched_for_movie(movie).await?,
                };

                outgoing.write(api::ListWatchedResponse { watched });
            }
            api::Request::MoveWatchedEpisode => {
                let req = incoming
                    .read::<api::MoveWatchedEpisodeRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .move_watched_episode(req.id, req.season, req.episode)
                    .await?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::WatchedChanged {
                        event: api::WatchedEvent::Episode {
                            show: req.show_id,
                            episode: api::EpisodeId::new(0),
                        },
                    },
                    "ws move watched episode changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::ListOrphanedWatched => {
                let req = incoming
                    .read::<api::ListOrphanedWatchedRequest>()
                    .context("Expected a request payload")?;

                let watched = self.db.orphaned_for_show(req.show_id).await?;
                outgoing.write(api::ListOrphanedWatchedResponse { watched });
            }
            api::Request::ListPending => {
                let _req = incoming
                    .read::<api::ListPendingRequest>()
                    .context("Expected a request payload")?;

                let now = self.pending_cutoff().await?;
                let pending = self.db.pending(now).await.context("Loading pending")?;
                outgoing.write(api::ListPendingResponse { pending });
            }
            api::Request::ListSchedule => {
                let req = incoming
                    .read::<api::ListScheduleRequest<'_>>()
                    .context("Expected a request payload")?;

                let tz = req.tz.and_then(TimeZone::get).unwrap_or(TimeZone::UTC);

                let time = api::TimeInfo::new(tz, api::Timestamp::now());
                let days = self
                    .db
                    .schedule(req.start_offset_days, req.days, time)
                    .await?;
                outgoing.write(api::ListScheduleResponse { days });
            }
            api::Request::ListWatchNext => {
                let _req = incoming
                    .read::<api::ListWatchNextRequest>()
                    .context("Expected a request payload")?;

                let now = self.pending_cutoff().await?;
                let pending = self.db.pending(now).await.context("Loading watch next")?;
                outgoing.write(api::ListWatchNextResponse { pending });
            }
            api::Request::Search => {
                let req = incoming
                    .read::<api::SearchRequest>()
                    .context("Expected a request payload")?;

                let mut shows: Vec<api::SearchShow> = Vec::new();
                let mut movies: Vec<api::SearchMovie> = Vec::new();

                // Search spans both kinds; query only the selected ones and sum
                // their totals so pagination accounts for every source.
                let mut total = 0;

                if req.shows {
                    let (results, count) = self.remote.search_show(&req.query, req.page).await?;

                    total += count;

                    for r in results {
                        let already_tracked =
                            self.db.shows_by_remote_id(&r.remote).await?.map(|s| s.id);

                        shows.push(api::SearchShow {
                            already_tracked,
                            ..r
                        });
                    }
                }

                if req.movies {
                    let (results, count) = self.remote.search_movies(&req.query, req.page).await?;

                    total += count;

                    for r in results {
                        let already_tracked =
                            self.db.movie_by_remote_id(&r.remote).await?.map(|m| m.id);

                        movies.push(api::SearchMovie {
                            already_tracked,
                            ..r
                        });
                    }
                }

                // Interleave the two kinds round-robin so results are mixed
                // through the list, while preserving each source's own order.
                let mut results = Vec::with_capacity(shows.len() + movies.len());
                let mut shows = shows.into_iter();
                let mut movies = movies.into_iter();

                loop {
                    let show = shows.next();
                    let movie = movies.next();

                    if show.is_none() && movie.is_none() {
                        break;
                    }

                    if let Some(show) = show {
                        results.push(api::SearchResult::Show(show));
                    }

                    if let Some(movie) = movie {
                        results.push(api::SearchResult::Movie(movie));
                    }
                }

                outgoing.write(api::SearchResponse { results, total });
            }
            api::Request::SyncShow => {
                let req = incoming
                    .read::<api::SyncShowRequest>()
                    .context("Expected a request payload")?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SyncEpisode => {
                let req = incoming
                    .read::<api::SyncEpisodeRequest>()
                    .context("Expected a request payload")?;

                let show = self
                    .db
                    .show_by_id(req.show_id)
                    .await?
                    .context("Expected show to exist")?;

                let episode = self
                    .db
                    .episode_by_id(req.episode_id)
                    .await?
                    .context("Expected episode to exist")?;

                self.enqueue_episode_sync(
                    show.id,
                    episode.id,
                    episode.code(),
                    show.strings.title().map(str::to_owned),
                    true,
                )
                .await;

                outgoing.write(api::Empty);
            }
            api::Request::SyncMovie => {
                let req = incoming
                    .read::<api::SyncMovieRequest>()
                    .context("Expected a request payload")?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SyncPerson => {
                let req = incoming
                    .read::<api::SyncPersonRequest>()
                    .context("Expected a request payload")?;

                let person = self
                    .db
                    .person_by_id(req.id)
                    .await?
                    .context("Expected person to exist")?;

                self.enqueue_person_sync(person.id, person.name.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SetShowRemoteEnabled => {
                let req = incoming
                    .read::<api::SetShowRemoteEnabledRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_show_remote_enabled(req.remote_id, req.enabled)
                    .await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws set show remote enabled changed",
                );

                self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::ReorderShowRemotes => {
                let req = incoming
                    .read::<api::ReorderShowRemotesRequest>()
                    .context("Expected a request payload")?;

                self.db.reorder_show_remotes(req.remote_ids).await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws reorder show remotes changed",
                );

                self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SetMovieRemoteEnabled => {
                let req = incoming
                    .read::<api::SetMovieRemoteEnabledRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_movie_remote_enabled(req.remote_id, req.enabled)
                    .await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws set movie remote enabled changed",
                );

                self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::ReorderMovieRemotes => {
                let req = incoming
                    .read::<api::ReorderMovieRemotesRequest>()
                    .context("Expected a request payload")?;

                self.db.reorder_movie_remotes(req.remote_ids).await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws reorder movie remotes changed",
                );

                self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SetShowRemoteSyncKinds => {
                let req = incoming
                    .read::<api::SetShowRemoteSyncKindsRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_show_remote_sync_kinds(req.remote_id, req.sync_kinds)
                    .await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws set show remote sync kinds changed",
                );

                self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SetMovieRemoteSyncKinds => {
                let req = incoming
                    .read::<api::SetMovieRemoteSyncKindsRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_movie_remote_sync_kinds(req.remote_id, req.sync_kinds)
                    .await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws set movie remote sync kinds changed",
                );

                self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::AddShowRemote => {
                let req = incoming
                    .read::<api::AddShowRemoteRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.db
                    .add_show_remote(req.id, req.slug.as_deref(), &req.remote)
                    .await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws add show remote changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws add show remote pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::RemoveShowRemote => {
                let req = incoming
                    .read::<api::RemoveShowRemoteRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.db.remove_show_remote(req.remote_id).await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws remove show remote changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws remove show remote pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::AddMovieRemote => {
                let req = incoming
                    .read::<api::AddMovieRemoteRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.db
                    .add_movie_remote(req.id, req.slug.as_deref(), &req.remote)
                    .await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws add movie remote changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws add movie remote pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::RemoveMovieRemote => {
                let req = incoming
                    .read::<api::RemoveMovieRemoteRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.db.remove_movie_remote(req.remote_id).await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws remove movie remote changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws remove movie remote pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::PurgeShowRemoteCache => {
                let req = incoming
                    .read::<api::PurgeShowRemoteCacheRequest>()
                    .context("Expected a request payload")?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.db.set_show_remote_cache(req.remote_id, None).await?;

                // The same remote also holds a validator on each of the show's
                // episodes; leaving those behind would let a "force resync" still be
                // answered from cache at the episode level.
                if let Some(entry) = show.remotes.iter().find(|e| e.id == req.remote_id) {
                    self.db
                        .clear_episode_cache_for_show_source(show.id, *entry.remote.source())
                        .await?;
                }

                // Force a fresh sync now that the cached validator is gone.
                self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
                    .await;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show },
                    "ws purge show remote cache",
                );

                outgoing.write(api::Empty);
            }
            api::Request::PurgeMovieRemoteCache => {
                let req = incoming
                    .read::<api::PurgeMovieRemoteCacheRequest>()
                    .context("Expected a request payload")?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.db.set_movie_remote_cache(req.remote_id, None).await?;

                self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
                    .await;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged { movie },
                    "ws purge movie remote cache",
                );

                outgoing.write(api::Empty);
            }
            api::Request::UpdateShowRemote => {
                let req = incoming
                    .read::<api::UpdateShowRemoteRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.db
                    .update_show_remote(req.remote_id, req.slug.as_deref(), &req.remote)
                    .await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws update show remote changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws update show remote pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::UpdateMovieRemote => {
                let req = incoming
                    .read::<api::UpdateMovieRemoteRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.db
                    .update_movie_remote(req.remote_id, req.slug.as_deref(), &req.remote)
                    .await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws update movie remote changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws update movie remote pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::SetShowLanguage => {
                let req = incoming
                    .read::<api::SetShowLanguageRequest>()
                    .context("Expected a request payload")?;

                self.db.set_show_language(req.id, req.language).await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws set show language changed",
                );

                self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SetShowIncludeSpecials => {
                let req = incoming
                    .read::<api::SetShowIncludeSpecialsRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_show_include_specials(req.id, req.include_specials)
                    .await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws set show include specials changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::SetShowAutoSync => {
                let req = incoming
                    .read::<api::SetShowAutoSyncRequest>()
                    .context("Expected a request payload")?;

                self.db.set_show_auto_sync(req.id, req.auto_sync).await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws set show auto sync changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::SetMovieAutoSync => {
                let req = incoming
                    .read::<api::SetMovieAutoSyncRequest>()
                    .context("Expected a request payload")?;

                self.db.set_movie_auto_sync(req.id, req.auto_sync).await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws set movie auto sync changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::SetShowAirDateFilters => {
                let req = incoming
                    .read::<api::SetShowAirDateFiltersRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_show_air_date_filters(req.id, req.air_date_filters)
                    .await?;

                // Recompute effective air dates against the new filters.
                let default = self.db.load_config().await?.air_date_filters;
                self.db
                    .recompute_episode_aired_for_show(req.id, default)
                    .await?;

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ShowChanged { show: show.clone() },
                    "ws set show air date filters changed",
                );

                for season in self.db.seasons(req.id).await? {
                    self.broadcast.emit(
                        incoming.channel(),
                        api::AppEventKind::EpisodesChanged {
                            show_id: req.id,
                            season: season.season,
                        },
                        "ws set show air date filters episodes changed",
                    );
                }

                outgoing.write(api::Empty);
            }
            api::Request::SetMovieLanguage => {
                let req = incoming
                    .read::<api::SetMovieLanguageRequest>()
                    .context("Expected a request payload")?;

                self.db.set_movie_language(req.id, req.language).await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws set movie language changed",
                );

                self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SetMovieReleaseFilters => {
                let req = incoming
                    .read::<api::SetMovieReleaseFiltersRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .set_movie_release_filters(req.id, req.release_filters)
                    .await?;

                // Recompute pending against the new filters and surface the updated movie.
                let default = self.db.load_config().await?.release_filters;
                self.db.update_movie_pending(req.id, default).await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws set movie release filters changed",
                );

                self.broadcast
                    .broadcast_event(api::AppEventKind::PendingChanged);

                outgoing.write(api::Empty);
            }
            api::Request::SyncAll => {
                let _req = incoming
                    .read::<api::SyncAllRequest>()
                    .context("Expected a request payload")?;

                let shows = self.db.shows().await?;

                for s in shows {
                    self.enqueue_show_sync(s.id, s.strings.title().map(str::to_owned), false)
                        .await;
                }

                let movies = self.db.movies().await?;

                for m in movies {
                    self.enqueue_movie_sync(m.id, m.strings.title().map(str::to_owned), false)
                        .await;
                }

                outgoing.write(api::Empty);
            }
            api::Request::ListTasks => {
                let _req = incoming
                    .read::<api::ListTasksRequest>()
                    .context("Expected a request payload")?;

                let tasks = self.queue.list().await;

                outgoing.write(tasks);
            }
            api::Request::RemoveTask => {
                let req = incoming
                    .read::<api::RemoveTaskRequest>()
                    .context("Expected a request payload")?;

                self.queue.remove(req.id, &self.broadcast).await;

                outgoing.write(api::Empty);
            }
            api::Request::BumpTask => {
                let req = incoming
                    .read::<api::BumpTaskRequest>()
                    .context("Expected a request payload")?;

                self.queue.bump(req.id, &self.broadcast).await;

                outgoing.write(api::Empty);
            }
            api::Request::GetConfig => {
                let _req = incoming
                    .read::<api::GetConfigRequest>()
                    .context("Expected a request payload")?;

                let config = self.db.load_config().await?;

                outgoing.write(api::GetConfigResponse { config });
            }
            api::Request::GetTopLanguages => {
                let _req = incoming
                    .read::<api::GetTopLanguagesRequest>()
                    .context("Expected a request payload")?;

                let top_languages = self.db.get_state_top_languages().await?;

                outgoing.write(api::GetTopLanguagesResponse { top_languages });
            }
            api::Request::SetConfig => {
                let req = incoming
                    .read::<api::SetConfigRequest>()
                    .context("Expected a request payload")?;

                let prev = self.db.load_config().await?;

                self.db.save_config(&req.config).await?;
                self.remote.configure(&req.config)?;

                self.config_changed.notify_one();

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ConfigChanged {
                        config: req.config.clone(),
                    },
                    "ws config changed",
                );

                // The global air-date filters feed every show's effective dates;
                // recompute when they change (per-show overrides use their own).
                if prev.air_date_filters != req.config.air_date_filters {
                    let default = req.config.air_date_filters.clone();

                    for show in self.db.shows().await? {
                        self.db
                            .recompute_episode_aired_for_show(show.id, default.clone())
                            .await?;
                    }

                    self.broadcast.emit(
                        incoming.channel(),
                        api::AppEventKind::PendingChanged,
                        "ws air date filters recompute",
                    );
                }

                // Likewise the global release filters feed every movie's stored
                // release date; recompute from existing data (no re-sync) when they
                // change (per-movie overrides use their own).
                if prev.release_filters != req.config.release_filters {
                    let default = req.config.release_filters.clone();

                    for movie in self.db.movies().await? {
                        self.db
                            .update_movie_pending(movie.id, default.clone())
                            .await?;
                    }

                    self.broadcast.emit(
                        incoming.channel(),
                        api::AppEventKind::PendingChanged,
                        "ws release filters recompute",
                    );
                }

                outgoing.write(api::Empty);
            }
            api::Request::AddPending => {
                let req = incoming
                    .read::<api::AddPendingRequest>()
                    .context("Expected a request payload")?;

                match req.kind {
                    api::PendingKind::Episode { show, episode } => {
                        let ts = match req.mark_time {
                            api::MarkTime::Now => api::Timestamp::now(),
                            api::MarkTime::At(ts) => ts,
                            api::MarkTime::WhenAired => {
                                let Some(aired) = self.db.episode_aired_by_id(episode).await?
                                else {
                                    anyhow::bail!("Episode does not have an aired date");
                                };

                                aired
                            }
                        };

                        self.db.add_pending_episode(show, episode, ts).await?;
                    }
                    api::PendingKind::Movie { movie } => {
                        let ts = match req.mark_time {
                            api::MarkTime::Now => api::Timestamp::now(),
                            api::MarkTime::At(ts) => ts,
                            api::MarkTime::WhenAired => {
                                let Some(released) = self.db.earliest_movie_release(movie).await?
                                else {
                                    anyhow::bail!("Movie does not have a release date");
                                };

                                released
                            }
                        };

                        self.db.add_pending_movie(movie, ts).await?;
                    }
                }

                // Rebuild just the affected entry so listeners can update a single
                // row instead of reloading the whole pending list.
                let pending = self.db.pending_entry(req.kind).await?;

                match pending.clone() {
                    Some(pending) => self.broadcast.emit(
                        incoming.channel(),
                        api::AppEventKind::PendingEntryChanged { pending },
                        "ws add pending",
                    ),
                    None => self.broadcast.emit(
                        incoming.channel(),
                        api::AppEventKind::PendingChanged,
                        "ws add pending",
                    ),
                }

                outgoing.write(api::AddPendingResponse { pending });
            }
            api::Request::RemovePending => {
                let req = incoming
                    .read::<api::RemovePendingRequest>()
                    .context("Expected a request payload")?;

                match req.kind {
                    api::PendingKind::Episode { show, .. } => {
                        self.db.remove_pending_episode(show).await?;
                    }
                    api::PendingKind::Movie { movie } => {
                        self.db.remove_pending_movie(movie).await?;
                    }
                }

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws remove pending",
                );

                outgoing.write(api::Empty);
            }
            api::Request::SkipEpisode => {
                let req = incoming
                    .read::<api::SkipEpisodeRequest>()
                    .context("Expected a request payload")?;

                self.db
                    .skip_pending_episode(req.show, req.episode, api::Timestamp::now())
                    .await?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws skip episode",
                );

                outgoing.write(api::Empty);
            }
            api::Request::SelectImage => {
                let req = incoming
                    .read::<api::SelectImageRequest>()
                    .context("Expected a request payload")?;

                let owner = self.db.select_image(req.id).await?;

                match owner {
                    api::ImageOwner::Show(show_id) => {
                        let show = self
                            .db
                            .show_by_id(show_id)
                            .await?
                            .context("Expected show to exist")?;

                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::ShowChanged { show: show.clone() },
                            "ws select image show changed",
                        );
                    }
                    api::ImageOwner::Movie(movie_id) => {
                        let movie = self
                            .db
                            .movie_by_id(movie_id)
                            .await?
                            .context("Expected movie to exist")?;

                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::MovieChanged {
                                movie: movie.clone(),
                            },
                            "ws select image movie changed",
                        );
                    }
                    api::ImageOwner::Season(season_id) => {
                        if let Some(show_id) = self.db.show_id_for_season(season_id).await? {
                            let seasons = self.db.seasons(show_id).await?;
                            self.broadcast.emit(
                                incoming.channel(),
                                api::AppEventKind::SeasonsChanged { show_id, seasons },
                                "ws select image season changed",
                            );
                        }
                    }
                }

                outgoing.write(api::Empty);
            }
            api::Request::ClearSelectedImage => {
                let req = incoming
                    .read::<api::ClearSelectedImageRequest>()
                    .context("Expected a request payload")?;

                self.db.clear_selected_image(req.owner, req.kind).await?;

                match req.owner {
                    api::ImageOwner::Show(show_id) => {
                        let show = self
                            .db
                            .show_by_id(show_id)
                            .await?
                            .context("Expected show to exist")?;

                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::ShowChanged { show: show.clone() },
                            "ws clear selected image show changed",
                        );
                    }
                    api::ImageOwner::Movie(movie_id) => {
                        let movie = self
                            .db
                            .movie_by_id(movie_id)
                            .await?
                            .context("Expected movie to exist")?;
                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::MovieChanged {
                                movie: movie.clone(),
                            },
                            "ws clear selected image movie changed",
                        );
                    }
                    api::ImageOwner::Season(season_id) => {
                        if let Some(show_id) = self.db.show_id_for_season(season_id).await? {
                            let seasons = self.db.seasons(show_id).await?;
                            self.broadcast.emit(
                                incoming.channel(),
                                api::AppEventKind::SeasonsChanged { show_id, seasons },
                                "ws clear selected image season changed",
                            );
                        }
                    }
                }

                outgoing.write(api::Empty);
            }
            api::Request::PickBestImages => {
                let req = incoming
                    .read::<api::PickBestImagesRequest>()
                    .context("Expected a request payload")?;

                match req.owner {
                    api::ImageOwner::Show(show_id) => {
                        self.db
                            .pick_best_show_image(show_id, req.kind, true)
                            .await?;

                        let show = self
                            .db
                            .show_by_id(show_id)
                            .await?
                            .context("Expected show to exist")?;

                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::ShowChanged { show: show.clone() },
                            "ws pick best images show changed",
                        );
                    }
                    api::ImageOwner::Movie(movie_id) => {
                        self.db
                            .pick_best_movie_image(movie_id, req.kind, true)
                            .await?;

                        let movie = self
                            .db
                            .movie_by_id(movie_id)
                            .await?
                            .context("Expected movie to exist")?;

                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::MovieChanged {
                                movie: movie.clone(),
                            },
                            "ws pick best images movie changed",
                        );
                    }
                    api::ImageOwner::Season(_) => {}
                }

                outgoing.write(api::Empty);
            }
            api::Request::ResetImageSelection => {
                let req = incoming
                    .read::<api::ResetImageSelectionRequest>()
                    .context("Expected a request payload")?;

                match req.owner {
                    api::ImageOwner::Show(show_id) => {
                        self.db
                            .pick_best_show_image(show_id, Some(req.kind), false)
                            .await?;

                        let show = self
                            .db
                            .show_by_id(show_id)
                            .await?
                            .context("Expected show to exist")?;

                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::ShowChanged { show: show.clone() },
                            "ws reset image selection show changed",
                        );
                    }
                    api::ImageOwner::Movie(movie_id) => {
                        self.db
                            .pick_best_movie_image(movie_id, Some(req.kind), false)
                            .await?;

                        let movie = self
                            .db
                            .movie_by_id(movie_id)
                            .await?
                            .context("Expected movie to exist")?;

                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::MovieChanged {
                                movie: movie.clone(),
                            },
                            "ws reset image selection movie changed",
                        );
                    }
                    api::ImageOwner::Season(_) => {}
                }

                outgoing.write(api::Empty);
            }
            api::Request::Unknown(id) => {
                anyhow::bail!("Unknown request id: {id:?}");
            }
        }

        Ok(())
    }
}

pub(super) async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> axum::response::Response {
    ws.on_upgrade(move |socket| async move {
        let handler = WsHandler {
            db: state.db.clone(),
            broadcast: state.broadcast.clone(),
            remote: state.remote.clone(),
            queue: state.queue.clone(),
            pending: state.pending.clone(),
            config_changed: state.config_changed.clone(),
            delay: state.delay,
        };

        let mut subscribe = state.broadcast.subscribe();

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
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    };

                    if let Err(error) = server.broadcast(msg) {
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
            }
        }
    })
}
