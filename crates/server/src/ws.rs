use core::iter;

use anyhow::{Context as _, Result};
use db::Database;
use musli_web::axum08;
use musli_web::ws;
use tokio::sync::broadcast;

use std::sync::Arc;

use crate::app_broadcast::Broadcaster;
use crate::pending::PendingSystem;
use crate::remote::RemoteClients;
use crate::task_queue::TaskQueue;

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
        tracing::debug!(?id, "request");
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
    async fn enqueue_series_sync(&self, series_id: api::SeriesId, title: String, immediate: bool) {
        self.queue
            .push(
                api::TaskKind::SyncSeries { series_id, title },
                immediate,
                &self.broadcast,
            )
            .await;
    }

    async fn enqueue_movie_sync(&self, movie_id: api::MovieId, title: String, immediate: bool) {
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
            api::Request::ListSeries => {
                let _req = incoming
                    .read::<api::ListSeriesRequest>()
                    .context("missing request")?;
                let series = self.db.series().await.context("loading series")?;
                outgoing.write(api::ListSeriesResponse { series });
            }
            api::Request::GetSeries => {
                let req = incoming
                    .read::<api::GetSeriesRequest>()
                    .context("missing request")?;

                let series = self
                    .db
                    .series_by_id(req.id)
                    .await?
                    .context("series not found")?;

                outgoing.write(series);
            }
            api::Request::ListSeasons => {
                let req = incoming
                    .read::<api::ListSeasonsRequest>()
                    .context("missing request")?;
                let seasons = self.db.seasons(req.series_id).await?;
                outgoing.write(api::ListSeasonsResponse { seasons });
            }
            api::Request::TrackSeries => {
                let req = incoming
                    .read::<api::TrackSeriesRequest>()
                    .context("missing request")?;

                let series = self
                    .db
                    .create_series(&req.remote_id.value().to_string(), None, "")
                    .await?;

                self.db.add_series_remote(series.id, &req.remote_id).await?;

                if let Some(source) = api::SyncSource::from_remote_source(req.remote_id.source()) {
                    self.db.set_series_sync_source(series.id, source).await?;
                }

                let series = self
                    .db
                    .series_by_id(series.id)
                    .await?
                    .context("series not found")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::SeriesCreated {
                        series: series.clone(),
                    },
                    "ws track series created",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws track series pending changed",
                );

                self.enqueue_series_sync(series.id, series.title.clone(), true)
                    .await;

                outgoing.write(series);
            }
            api::Request::UntrackSeries => {
                let req = incoming
                    .read::<api::UntrackSeriesRequest>()
                    .context("missing request")?;
                self.db.set_series_tracked(req.id, req.tracked).await?;
                let series = self
                    .db
                    .series_by_id(req.id)
                    .await?
                    .context("series not found")?;
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::SeriesChanged {
                        series: series.clone(),
                    },
                    "ws untrack series changed",
                );
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws untrack series pending changed",
                );
                outgoing.write(api::Empty);
            }
            api::Request::RemoveSeries => {
                let req = incoming
                    .read::<api::RemoveSeriesRequest>()
                    .context("missing request")?;
                self.db.delete_series(req.id).await?;
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::SeriesDeleted { series_id: req.id },
                    "ws remove series deleted",
                );
                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws remove series pending changed",
                );
                outgoing.write(api::Empty);
            }
            api::Request::ListEpisodes => {
                let req = incoming
                    .read::<api::ListEpisodesRequest>()
                    .context("missing request")?;
                let episodes = self.db.episodes(req.series_id, req.season).await?;
                outgoing.write(api::ListEpisodesResponse { episodes });
            }
            api::Request::ListMovies => {
                let _req = incoming
                    .read::<api::ListMoviesRequest>()
                    .context("missing request")?;
                let movies = self.db.movies().await?;
                outgoing.write(api::ListMoviesResponse { movies });
            }
            api::Request::GetMovie => {
                let req = incoming
                    .read::<api::GetMovieRequest>()
                    .context("missing request")?;
                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("movie not found")?;
                outgoing.write(movie);
            }
            api::Request::TrackMovie => {
                let req = incoming
                    .read::<api::TrackMovieRequest>()
                    .context("missing request")?;

                let movie = self
                    .db
                    .create_movie(&req.remote_id.value().to_string(), None, "", true)
                    .await?;

                self.db.add_movie_remote(movie.id, &req.remote_id).await?;

                if let Some(source) = api::SyncSource::from_remote_source(req.remote_id.source()) {
                    self.db.set_movie_sync_source(movie.id, source).await?;
                }

                let movie = self
                    .db
                    .movie_by_id(movie.id)
                    .await?
                    .context("movie not found")?;

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
                    .context("missing request")?;
                self.db.set_movie_tracked(req.id, req.tracked).await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("movie not found")?;

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
                    .context("missing request")?;

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
                    .context("missing request")?;

                let ts = req.timestamp.unwrap_or_else(api::Timestamp::now);

                let watched = self.db.mark_watched(req.kind, ts).await?;

                if let api::WatchedKind::Episode { series, .. } = req.kind {
                    self.pending.on_episode_watched(series).await?;
                }

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::WatchedChanged { kind: req.kind },
                    "ws mark watched changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws mark watched pending changed",
                );

                outgoing.write(api::MarkWatchedResponse { watched });
            }
            api::Request::RemoveWatched => {
                let req = incoming
                    .read::<api::RemoveWatchedRequest>()
                    .context("missing request")?;

                self.db.remove_watched(req.id).await?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::WatchedChanged { kind: req.kind },
                    "ws remove watched changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws remove watched pending changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::ListWatched => {
                let req = incoming
                    .read::<api::ListWatchedRequest>()
                    .context("missing request")?;

                let watched = match req.kind {
                    api::WatchedKind::Episode { episode, .. } => {
                        self.db.watched_for_episode(episode).await?
                    }
                    api::WatchedKind::Movie { movie } => self.db.watched_for_movie(movie).await?,
                };

                outgoing.write(api::ListWatchedResponse { watched });
            }
            api::Request::ListPending => {
                let _req = incoming
                    .read::<api::ListPendingRequest>()
                    .context("missing request")?;

                let pending = self.db.pending().await.context("loading pending")?;

                outgoing.write(api::ListPendingResponse { pending });
            }
            api::Request::ListSchedule => {
                let req = incoming
                    .read::<api::ListScheduleRequest>()
                    .context("missing request")?;

                let days = self.db.schedule(req.days).await?;

                outgoing.write(api::ListScheduleResponse { days });
            }
            api::Request::ListWatchNext => {
                let _req = incoming
                    .read::<api::ListWatchNextRequest>()
                    .context("missing request")?;

                let mut pending = self.db.pending().await.context("loading watch next")?;

                pending.retain(|p| matches!(p.kind, api::PendingKind::Episode { .. }));

                outgoing.write(api::ListWatchNextResponse { pending });
            }
            api::Request::Search => {
                let req = incoming
                    .read::<api::SearchRequest>()
                    .context("missing request")?;

                let mut series: Vec<api::SearchSeries> = Vec::new();
                let mut movies: Vec<api::SearchMovie> = Vec::new();

                match req.kind {
                    api::SearchKind::Series => {
                        for r in self.remote.search_series(&req.query).await? {
                            let already_tracked = self
                                .db
                                .series_by_remote_id(&r.remote_id)
                                .await?
                                .map(|s| s.id);

                            series.push(api::SearchSeries {
                                already_tracked,
                                ..r
                            });
                        }
                    }
                    api::SearchKind::Movies => {
                        for r in self.remote.search_movies(&req.query).await? {
                            let already_tracked = self
                                .db
                                .movie_by_remote_id(&r.remote_id)
                                .await?
                                .map(|m| m.id);

                            movies.push(api::SearchMovie {
                                already_tracked,
                                ..r
                            });
                        }
                    }
                }

                outgoing.write(api::SearchResponse { series, movies });
            }
            api::Request::SyncSeries => {
                let req = incoming
                    .read::<api::SyncSeriesRequest>()
                    .context("missing request")?;

                let series = self
                    .db
                    .series_by_id(req.id)
                    .await?
                    .context("series not found")?;

                self.enqueue_series_sync(series.id, series.title, true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SyncMovie => {
                let req = incoming
                    .read::<api::SyncMovieRequest>()
                    .context("missing request")?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("movie not found")?;

                self.enqueue_movie_sync(movie.id, movie.title, true).await;

                outgoing.write(api::Empty);
            }
            api::Request::SetSeriesSyncSource => {
                let req = incoming
                    .read::<api::SetSeriesSyncSourceRequest>()
                    .context("missing request")?;

                let series = self
                    .db
                    .series_by_id(req.id)
                    .await?
                    .context("series not found")?;

                if series.remote_by_source(req.source.as_str()).is_none() {
                    anyhow::bail!("series does not have remote for source: {}", req.source);
                }

                self.db.set_series_sync_source(req.id, req.source).await?;

                let series = self
                    .db
                    .series_by_id(req.id)
                    .await?
                    .context("series not found")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::SeriesChanged {
                        series: series.clone(),
                    },
                    "ws set series sync source changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws set series sync source pending changed",
                );

                self.enqueue_series_sync(series.id, series.title, true)
                    .await;

                outgoing.write(api::Empty);
            }
            api::Request::SetMovieSyncSource => {
                let req = incoming
                    .read::<api::SetMovieSyncSourceRequest>()
                    .context("missing request")?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("movie not found")?;

                if req.source != api::SyncSource::Tmdb {
                    anyhow::bail!("unsupported movie sync source: {}", req.source);
                }

                if movie.remote_by_source(req.source.as_str()).is_none() {
                    anyhow::bail!("movie does not have remote for source: {}", req.source);
                }

                self.db.set_movie_sync_source(req.id, req.source).await?;

                let movie = self
                    .db
                    .movie_by_id(req.id)
                    .await?
                    .context("movie not found")?;

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::MovieChanged {
                        movie: movie.clone(),
                    },
                    "ws set movie sync source changed",
                );

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::PendingChanged,
                    "ws set movie sync source pending changed",
                );

                self.enqueue_movie_sync(movie.id, movie.title, true).await;

                outgoing.write(api::Empty);
            }
            api::Request::SyncAll => {
                let _req = incoming
                    .read::<api::SyncAllRequest>()
                    .context("missing request")?;

                let series = self.db.series().await?;

                for s in series {
                    self.enqueue_series_sync(s.id, s.title, false).await;
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
                    .context("missing request")?;

                let tasks = self.queue.list().await;

                outgoing.write(tasks);
            }
            api::Request::GetConfig => {
                let _req = incoming
                    .read::<api::GetConfigRequest>()
                    .context("missing request")?;

                let config = self.db.load_config().await?;

                outgoing.write(api::GetConfigResponse { config });
            }
            api::Request::SetConfig => {
                let req = incoming
                    .read::<api::SetConfigRequest>()
                    .context("missing request")?;

                self.db.save_config(&req.config).await?;

                self.remote.configure(&req.config);

                self.config_changed.notify_one();

                self.broadcast.emit(
                    incoming.channel(),
                    api::AppEventKind::ConfigChanged {
                        config: req.config.clone(),
                    },
                    "ws config changed",
                );

                outgoing.write(api::Empty);
            }
            api::Request::AddPending => {
                let req = incoming
                    .read::<api::AddPendingRequest>()
                    .context("missing request")?;

                let ts = api::Timestamp::now();

                match req.kind {
                    api::PendingKind::Episode { series, episode } => {
                        self.db.add_pending_episode(series, episode, ts).await?;
                    }
                    api::PendingKind::Movie { movie } => {
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
                    .context("missing request")?;

                match req.kind {
                    api::PendingKind::Episode { series, .. } => {
                        self.db.remove_pending_episode(series).await?;
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
            api::Request::SelectImage => {
                let req = incoming
                    .read::<api::SelectImageRequest>()
                    .context("missing request")?;

                let owner = self.db.select_image(req.id).await?;

                match owner {
                    api::ImageOwner::Series(series_id) => {
                        let series = self
                            .db
                            .series_by_id(series_id)
                            .await?
                            .context("series not found")?;
                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::SeriesChanged {
                                series: series.clone(),
                            },
                            "ws select image series changed",
                        );
                    }
                    api::ImageOwner::Movie(movie_id) => {
                        let movie = self
                            .db
                            .movie_by_id(movie_id)
                            .await?
                            .context("movie not found")?;
                        self.broadcast.emit(
                            incoming.channel(),
                            api::AppEventKind::MovieChanged {
                                movie: movie.clone(),
                            },
                            "ws select image movie changed",
                        );
                    }
                }

                outgoing.write(api::Empty);
            }
            api::Request::Unknown(id) => {
                anyhow::bail!("unknown request id: {id:?}");
            }
        }

        Ok(())
    }
}

pub(super) async fn ws_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<crate::AppState>,
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
