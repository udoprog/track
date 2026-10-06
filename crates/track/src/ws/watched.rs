use anyhow::{Context as _, Result};
use api::TimeZone;
use musli_web::ws;

use super::WsHandler;

impl WsHandler {
    pub(super) async fn mark_watched_request(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::MarkWatchedRequest>()
            .context("Expected a request payload")?;

        let response = self
            .mark_watched(
                incoming.channel(),
                req.kind,
                req.mark_time,
                api::Timestamp::now(),
            )
            .await?;
        outgoing.write(response);
        Ok(())
    }

    pub(super) async fn mark_next_episode(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::MarkNextEpisodeRequest>()
            .context("Expected a request payload")?;

        let now = api::Timestamp::now();

        let marked = match self
            .db
            .next_episode(self.user.id, req.show, req.scope, now)
            .await?
        {
            Some(episode) => {
                let kind = api::WatchedKind::Episode {
                    show: req.show,
                    episode,
                };

                Some(
                    self.mark_watched(incoming.channel(), kind, req.mark_time, now)
                        .await?,
                )
            }
            None => None,
        };

        outgoing.write(api::MarkNextEpisodeResponse { marked });
        Ok(())
    }

    pub(super) async fn mark_watched_remaining(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::MarkWatchedRemainingRequest>()
            .context("Expected a request payload")?;

        let now = api::Timestamp::now();

        let last = self
            .db
            .mark_watched_remaining(self.user.id, req.show_id, req.season, req.mark_time, now)
            .await?;

        if let Some(last) = last {
            self.pending
                .on_episode_watched_from(self.user.id, req.show_id, last, now)
                .await?;
        }

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::WatchedChanged {
                event: api::WatchedEvent::RemainingSeason {
                    show: req.show_id,
                    season: req.season,
                },
            },
            "ws mark watched changed remaining season",
        );

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws mark watched remaining pending changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn remove_watched(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::RemoveWatchedRequest>()
            .context("Expected a request payload")?;

        self.db.remove_watched(self.user.id, req.id).await?;

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::WatchedChanged {
                event: req.kind.into_event(),
            },
            "ws remove watched changed",
        );

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws remove watched pending changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn undo_watched(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::UndoWatchedRequest>()
            .context("Expected a request payload")?;

        self.db.remove_watched(self.user.id, req.id).await?;

        match (req.kind, req.pending_before) {
            (
                api::WatchedKind::Episode { show, .. },
                api::PendingBefore::Episode { episode, timestamp },
            ) => {
                self.db
                    .add_pending_episode(self.user.id, show, episode, timestamp)
                    .await?;
            }
            (api::WatchedKind::Episode { show, .. }, _) => {
                self.db.remove_pending_episode(self.user.id, show).await?;
            }
            (api::WatchedKind::Movie { movie }, api::PendingBefore::Movie { timestamp }) => {
                self.db
                    .add_pending_movie(self.user.id, movie, timestamp)
                    .await?;
            }
            (api::WatchedKind::Movie { .. }, _) => {}
        }

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::WatchedChanged {
                event: req.kind.into_event(),
            },
            "ws undo watched changed",
        );

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws undo watched pending changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn list_episodes_watched(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ListEpisodesWatchedRequest>()
            .context("Expected a request payload")?;

        let watched = self.db.episodes_watched(self.user.id, req.show_id).await?;
        outgoing.write(api::ListEpisodesWatchedResponse { watched });
        Ok(())
    }

    pub(super) async fn list_watched(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ListWatchedRequest>()
            .context("Expected a request payload")?;

        let watched = match req.kind {
            api::WatchedKind::Episode { episode, .. } => {
                self.db.watched_for_episode(self.user.id, episode).await?
            }
            api::WatchedKind::Movie { movie } => {
                self.db.watched_for_movie(self.user.id, movie).await?
            }
        };

        outgoing.write(api::ListWatchedResponse { watched });
        Ok(())
    }

    pub(super) async fn move_watched_episode(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::MoveWatchedEpisodeRequest>()
            .context("Expected a request payload")?;

        self.db
            .move_watched_episode(self.user.id, req.id, req.season, req.episode)
            .await?;

        self.broadcast.emit_to(
            self.user.id,
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
        Ok(())
    }

    pub(super) async fn list_orphaned_watched(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ListOrphanedWatchedRequest>()
            .context("Expected a request payload")?;

        let watched = self.db.orphaned_for_show(self.user.id, req.show_id).await?;
        outgoing.write(api::ListOrphanedWatchedResponse { watched });
        Ok(())
    }

    pub(super) async fn list_pending(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let _req = incoming
            .read::<api::ListPendingRequest>()
            .context("Expected a request payload")?;

        let now = self.pending_cutoff().await?;
        let pending = self
            .db
            .pending(self.user.id, now)
            .await
            .context("Loading pending")?;
        outgoing.write(api::ListPendingResponse { pending });
        Ok(())
    }

    pub(super) async fn list_schedule(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ListScheduleRequest<'_>>()
            .context("Expected a request payload")?;

        let tz = req.tz.and_then(TimeZone::get).unwrap_or(TimeZone::UTC);

        let time = api::TimeInfo::new(tz, api::Timestamp::now());
        let days = self
            .db
            .schedule(self.user.id, req.start_offset_days, req.days, time)
            .await?;
        outgoing.write(api::ListScheduleResponse { days });
        Ok(())
    }

    pub(super) async fn list_watch_next(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let _req = incoming
            .read::<api::ListWatchNextRequest>()
            .context("Expected a request payload")?;

        let now = self.pending_cutoff().await?;
        let pending = self
            .db
            .pending(self.user.id, now)
            .await
            .context("Loading watch next")?;
        outgoing.write(api::ListWatchNextResponse { pending });
        Ok(())
    }

    pub(super) async fn add_pending(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::AddPendingRequest>()
            .context("Expected a request payload")?;

        match req.kind {
            api::PendingKind::Episode { show, episode } => {
                let ts = match req.mark_time {
                    api::MarkTime::Now => api::Timestamp::now(),
                    api::MarkTime::At(ts) => ts,
                    api::MarkTime::WhenAired => {
                        let Some(aired) = self.db.episode_aired_by_id(episode).await? else {
                            anyhow::bail!("Episode does not have an aired date");
                        };

                        aired
                    }
                };

                self.db
                    .add_pending_episode(self.user.id, show, episode, ts)
                    .await?;
            }
            api::PendingKind::Movie { movie } => {
                let ts = match req.mark_time {
                    api::MarkTime::Now => api::Timestamp::now(),
                    api::MarkTime::At(ts) => ts,
                    api::MarkTime::WhenAired => {
                        let Some(released) = self.db.earliest_movie_release(movie).await? else {
                            anyhow::bail!("Movie does not have a release date");
                        };

                        released
                    }
                };

                self.db.add_pending_movie(self.user.id, movie, ts).await?;
            }
        }

        // Rebuild just the affected entry so listeners can update a single
        // row instead of reloading the whole pending list.
        let pending = self.db.pending_entry(self.user.id, req.kind).await?;

        match pending.clone() {
            Some(pending) => self.broadcast.emit_to(
                self.user.id,
                incoming.channel(),
                api::AppEventKind::PendingEntryChanged { pending },
                "ws add pending",
            ),
            None => self.broadcast.emit_to(
                self.user.id,
                incoming.channel(),
                api::AppEventKind::PendingChanged,
                "ws add pending",
            ),
        }

        outgoing.write(api::AddPendingResponse { pending });
        Ok(())
    }

    pub(super) async fn remove_pending(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::RemovePendingRequest>()
            .context("Expected a request payload")?;

        match req.kind {
            api::PendingKind::Episode { show, .. } => {
                self.db.remove_pending_episode(self.user.id, show).await?;
            }
            api::PendingKind::Movie { movie } => {
                self.db.remove_pending_movie(self.user.id, movie).await?;
            }
        }

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws remove pending",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn skip_episode(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SkipEpisodeRequest>()
            .context("Expected a request payload")?;

        self.db
            .skip_pending_episode(self.user.id, req.show, req.episode, api::Timestamp::now())
            .await?;

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws skip episode",
        );

        outgoing.write(api::Empty);
        Ok(())
    }
}
