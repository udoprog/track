use core::iter;

use anyhow::{Context as _, Result};
use db::Database;
use musli_web::axum08;
use musli_web::ws;
use tokio::sync::broadcast;

#[derive(Clone)]
pub(super) struct WsHandler {
    pub(super) db: Database,
    pub(super) broadcast: broadcast::Sender<api::AppEvent>,
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
                    .create_series(
                        &req.remote_id
                            .as_str()
                            .split(':')
                            .last()
                            .unwrap_or("Unknown"),
                        None,
                        "",
                        None,
                        None,
                        None,
                        Some(&req.remote_id),
                    )
                    .await?;
                let _ = self.broadcast.send(api::AppEvent {
                    channel: incoming.channel(),
                    kind: api::AppEventKind::SeriesCreated {
                        series: series.clone(),
                    },
                });
                let _ = self.broadcast.send(api::AppEvent {
                    channel: musli_web::api::ChannelId::NONE,
                    kind: api::AppEventKind::PendingChanged,
                });
                outgoing.write(series);
            }
            api::Request::UntrackSeries => {
                let req = incoming
                    .read::<api::UntrackSeriesRequest>()
                    .context("missing request")?;
                self.db.set_series_tracked(req.id, false).await?;
                let series = self
                    .db
                    .series_by_id(req.id)
                    .await?
                    .context("series not found")?;
                let _ = self.broadcast.send(api::AppEvent {
                    channel: incoming.channel(),
                    kind: api::AppEventKind::SeriesChanged {
                        series: series.clone(),
                    },
                });
                let _ = self.broadcast.send(api::AppEvent {
                    channel: musli_web::api::ChannelId::NONE,
                    kind: api::AppEventKind::PendingChanged,
                });
                outgoing.write(api::Empty);
            }
            api::Request::RemoveSeries => {
                let req = incoming
                    .read::<api::RemoveSeriesRequest>()
                    .context("missing request")?;
                self.db.delete_series(req.id).await?;
                let _ = self.broadcast.send(api::AppEvent {
                    channel: incoming.channel(),
                    kind: api::AppEventKind::SeriesDeleted { series_id: req.id },
                });
                let _ = self.broadcast.send(api::AppEvent {
                    channel: musli_web::api::ChannelId::NONE,
                    kind: api::AppEventKind::PendingChanged,
                });
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
                    .create_movie(
                        req.remote_id
                            .as_str()
                            .split(':')
                            .last()
                            .unwrap_or("Unknown"),
                        None,
                        "",
                        None,
                        None,
                        None,
                        Some(&req.remote_id),
                    )
                    .await?;
                let _ = self.broadcast.send(api::AppEvent {
                    channel: incoming.channel(),
                    kind: api::AppEventKind::MovieCreated {
                        movie: movie.clone(),
                    },
                });
                let _ = self.broadcast.send(api::AppEvent {
                    channel: musli_web::api::ChannelId::NONE,
                    kind: api::AppEventKind::PendingChanged,
                });
                outgoing.write(movie);
            }
            api::Request::RemoveMovie => {
                let req = incoming
                    .read::<api::RemoveMovieRequest>()
                    .context("missing request")?;
                self.db.delete_movie(req.id).await?;
                let _ = self.broadcast.send(api::AppEvent {
                    channel: incoming.channel(),
                    kind: api::AppEventKind::MovieDeleted { movie_id: req.id },
                });
                let _ = self.broadcast.send(api::AppEvent {
                    channel: musli_web::api::ChannelId::NONE,
                    kind: api::AppEventKind::PendingChanged,
                });
                outgoing.write(api::Empty);
            }
            api::Request::MarkWatched => {
                let req = incoming
                    .read::<api::MarkWatchedRequest>()
                    .context("missing request")?;
                let ts = req.timestamp.unwrap_or_else(api::Timestamp::now);
                let watched = self.db.mark_watched(req.kind, ts).await?;
                let _ = self.broadcast.send(api::AppEvent {
                    channel: incoming.channel(),
                    kind: api::AppEventKind::WatchedChanged { kind: req.kind },
                });
                let _ = self.broadcast.send(api::AppEvent {
                    channel: musli_web::api::ChannelId::NONE,
                    kind: api::AppEventKind::PendingChanged,
                });
                outgoing.write(api::MarkWatchedResponse { watched });
            }
            api::Request::RemoveWatched => {
                let req = incoming
                    .read::<api::RemoveWatchedRequest>()
                    .context("missing request")?;
                self.db.remove_watched(req.id).await?;
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

                let config = self
                    .db
                    .load_config()
                    .await
                    .context("loading configuration")?;
                let mut pending = self
                    .db
                    .pending_episodes(config.dashboard_limit)
                    .await
                    .context("loading pending episodes")?;
                pending.extend(
                    self.db
                        .pending_movies()
                        .await
                        .context("loading pending movies")?,
                );
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
                let config = self.db.load_config().await?;
                let pending = self.db.pending_episodes(config.dashboard_page).await?;
                outgoing.write(api::ListWatchNextResponse { pending });
            }
            api::Request::Search => {
                let _req = incoming
                    .read::<api::SearchRequest>()
                    .context("missing request")?;
                outgoing.write(api::SearchResponse {
                    series: Vec::new(),
                    movies: Vec::new(),
                });
            }
            api::Request::SyncSeries => {
                let _req = incoming
                    .read::<api::SyncSeriesRequest>()
                    .context("missing request")?;
                outgoing.write(api::Empty);
            }
            api::Request::SyncAll => {
                let _req = incoming
                    .read::<api::SyncAllRequest>()
                    .context("missing request")?;
                outgoing.write(api::Empty);
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
                let _ = self.broadcast.send(api::AppEvent {
                    channel: incoming.channel(),
                    kind: api::AppEventKind::ConfigChanged {
                        config: req.config.clone(),
                    },
                });
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
