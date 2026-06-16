use core::iter;

use std::sync::Arc;

use anyhow::{Context as _, Result};
use api::{MovieId, ShowId, TimeZone};
use axum::extract::State;
use axum::extract::WebSocketUpgrade;
use musli_web::axum08;
use musli_web::ws;
use tokio::sync::broadcast;

use crate::app_broadcast::Broadcaster;
use crate::db::Database;
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::task_queue::TaskQueue;
use crate::web::AppState;

#[derive(Clone)]
pub(super) struct WsHandler {
    pub(super) db: Database,
    pub(super) broadcast: Broadcaster,
    pub(super) remote: RemoteClients,
    pub(super) queue: TaskQueue,
    pub(super) pending: PendingSystem,
    pub(super) config_changed: Arc<tokio::sync::Notify>,
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

                let show = self
                    .db
                    .show_by_id(req.id)
                    .await?
                    .context("Expected show to exist")?;

                outgoing.write(show);
            }
            api::Request::ListSeasons => {
                let req = incoming
                    .read::<api::ListSeasonsRequest>()
                    .context("Expected a request payload")?;
                let seasons = self.db.seasons(req.show_id).await?;
                outgoing.write(api::ListSeasonsResponse { seasons });
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

                self.enqueue_show_sync(show.id, show.title.clone(), true)
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
            api::Request::GetMovie => {
                let req = incoming
                    .read::<api::GetMovieRequest>()
                    .context("Expected a request payload")?;
                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("Expected movie to exist")?;
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

                self.enqueue_movie_sync(movie.id, movie.title.clone(), true)
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
                let watched = self
                    .db
                    .mark_watched(api::WatchedId::random(), req.kind, req.mark_time, now)
                    .await?;

                if let api::WatchedKind::Episode { show, episode } = req.kind {
                    self.pending
                        .on_episode_watched_from(show, episode, now)
                        .await?;
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

                outgoing.write(api::MarkWatchedResponse { watched });
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

                let now = api::Timestamp::now();
                let pending = self.db.pending(now).await.context("Loading pending")?;
                outgoing.write(api::ListPendingResponse { pending });
            }
            api::Request::ListSchedule => {
                let req = incoming
                    .read::<api::ListScheduleRequest>()
                    .context("Expected a request payload")?;

                let tz = req
                    .tz
                    .as_deref()
                    .and_then(TimeZone::get)
                    .unwrap_or(TimeZone::UTC);

                let now = api::Timestamp::now();

                let days = self.db.schedule(req.days, now, tz).await?;
                outgoing.write(api::ListScheduleResponse { days });
            }
            api::Request::ListWatchNext => {
                let _req = incoming
                    .read::<api::ListWatchNextRequest>()
                    .context("Expected a request payload")?;

                let now = api::Timestamp::now();
                let pending = self.db.pending(now).await.context("Loading watch next")?;
                outgoing.write(api::ListWatchNextResponse { pending });
            }
            api::Request::Search => {
                let req = incoming
                    .read::<api::SearchRequest>()
                    .context("Expected a request payload")?;

                let mut shows: Vec<api::SearchShow> = Vec::new();
                let mut movies: Vec<api::SearchMovie> = Vec::new();

                let total;

                match req.kind {
                    api::SearchKind::Show => {
                        let (results, count) =
                            self.remote.search_show(&req.query, req.page).await?;

                        total = count;

                        for r in results {
                            let already_tracked =
                                self.db.shows_by_remote_id(&r.remote).await?.map(|s| s.id);

                            shows.push(api::SearchShow {
                                already_tracked,
                                ..r
                            });
                        }
                    }
                    api::SearchKind::Movies => {
                        let (results, count) =
                            self.remote.search_movies(&req.query, req.page).await?;

                        total = count;

                        for r in results {
                            let already_tracked =
                                self.db.movie_by_remote_id(&r.remote).await?.map(|m| m.id);

                            movies.push(api::SearchMovie {
                                already_tracked,
                                ..r
                            });
                        }
                    }
                }

                outgoing.write(api::SearchResponse {
                    shows,
                    movies,
                    total,
                });
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

                self.enqueue_show_sync(show.id, show.title, true).await;

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

                self.enqueue_movie_sync(movie.id, movie.title, true).await;

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

                self.enqueue_show_sync(show.id, show.title, true).await;

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

                self.enqueue_show_sync(show.id, show.title, true).await;

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

                self.enqueue_movie_sync(movie.id, movie.title, true).await;

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

                self.enqueue_movie_sync(movie.id, movie.title, true).await;

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

                self.enqueue_show_sync(show.id, show.title, true).await;

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

                self.enqueue_movie_sync(movie.id, movie.title, true).await;

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
                crate::background::update_movie_pending(&self.db, req.id).await?;

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
                    self.enqueue_show_sync(s.id, s.title, false).await;
                }

                let movies = self.db.movies().await?;

                for m in movies {
                    self.enqueue_movie_sync(m.id, m.title, false).await;
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

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws add pending",
                );

                outgoing.write(api::Empty);
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

                self.db.skip_pending_episode(req.show, req.episode).await?;

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
        };

        let mut subscribe = state.broadcast.subscribe();
        let mut server =
            axum08::server(socket, handler).with_channel_allocator(state.channels.clone());

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
