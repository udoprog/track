use anyhow::{Context as _, Result};
use api::ShowId;
use musli_web::ws;

use super::WsHandler;
use crate::xem;

impl WsHandler {
    pub(super) async fn get_show(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::GetShowRequest>()
            .context("Expected a request payload")?;

        let show = self.db.show_by_id(Some(self.user.id), req.id).await?;

        outgoing.write(show);
        Ok(())
    }

    pub(super) async fn list_seasons(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ListSeasonsRequest>()
            .context("Expected a request payload")?;
        let mut seasons = self.db.seasons(Some(self.user.id), req.show_id).await?;
        self.add_season_names(req.show_id, &mut seasons).await?;
        outgoing.write(api::ListSeasonsResponse { seasons });
        Ok(())
    }

    pub(super) async fn get_season_images(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::GetSeasonImagesRequest>()
            .context("Expected a request payload")?;
        let images = self.db.season_images_by_id(req.season_id).await?;
        outgoing.write(api::GetSeasonImagesResponse { images });
        Ok(())
    }

    pub(super) async fn track_show(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::TrackShowRequest>()
            .context("Expected a request payload")?;

        let existing = self.db.show_id_by_remote(&req.remote).await?;
        let show_id = existing.unwrap_or_else(ShowId::random);

        if existing.is_none() {
            self.db
                .create_show(show_id, &req.remote.value().to_string(), None, "")
                .await?;
        }

        self.db
            .add_remote(show_id, req.slug.as_deref(), &req.remote)
            .await?;

        self.db
            .set_show_tracked(self.user.id, show_id, true)
            .await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), show_id)
            .await?
            .context("Expected show to exist")?;

        if existing.is_some() {
            self.fill_my_pending(&show).await?;

            self.broadcast.emit_to(
                self.user.id,
                incoming.channel(),
                api::AppEventKind::ShowCreated { show: show.clone() },
                "ws track existing show",
            );
        } else {
            self.broadcast.emit(
                incoming.channel(),
                api::AppEventKind::ShowCreated { show: show.clone() },
                "ws track show created",
            );
        }

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws track show pending changed",
        );

        self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
            .await;

        outgoing.write(show);
        Ok(())
    }

    pub(super) async fn untrack_show(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::UntrackShowRequest>()
            .context("Expected a request payload")?;
        self.db
            .set_show_tracked(self.user.id, req.id, req.tracked)
            .await?;
        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        if req.tracked {
            self.fill_my_pending(&show).await?;
        }
        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::ShowChanged { show: show.clone() },
            "ws untrack show changed",
        );
        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws untrack show pending changed",
        );
        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn remove_show(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
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
        Ok(())
    }

    pub(super) async fn list_episodes(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ListEpisodesRequest>()
            .context("Expected a request payload")?;
        let mut episodes = self
            .db
            .episodes(self.user.id, req.show_id, req.season)
            .await?;
        self.add_numberings(req.show_id, &mut episodes).await?;
        let watched = self.db.episodes_watched(self.user.id, req.show_id).await?;
        outgoing.write(api::ListEpisodesResponse { episodes, watched });
        Ok(())
    }

    /// Link each episode to XEM and fill in its other numberings.
    async fn add_numberings(&self, show_id: ShowId, episodes: &mut [api::Episode]) -> Result<()> {
        let Some(show) = self.db.show_by_id(None, show_id).await? else {
            return Ok(());
        };

        let config = self.db.load_config().await?;
        let base = api::base_source(&show.remotes, &config);

        let targets = episodes
            .iter()
            .map(|e| xem::link_target(base, show.numbering.as_ref(), e.season.as_u32(), e.episode))
            .collect::<Vec<_>>();

        let entries = self.db.xem_entries(show_id, targets.clone()).await?;

        for ((episode, target), entry) in episodes.iter_mut().zip(targets).zip(entries) {
            // XEM doesn't map the episode: nothing to link or show.
            let Some(target) = target.filter(|_| !entry.is_empty()) else {
                continue;
            };

            episode.numberings = xem::alternatives(
                &config.numberings,
                episode.season.as_u32(),
                episode.episode,
                &entry,
            );

            episode.link = Some(api::LinkedSeason {
                system: target.system,
                season: target.season,
            });
        }

        Ok(())
    }

    /// Fill in the XEM names of the seasons each season is linked to.
    async fn add_season_names(&self, show_id: ShowId, seasons: &mut [api::Season]) -> Result<()> {
        let Some(show) = self.db.show_by_id(None, show_id).await? else {
            return Ok(());
        };

        let config = self.db.load_config().await?;
        let base = api::base_source(&show.remotes, &config);

        let targets = seasons
            .iter()
            .map(|s| xem::season_targets(base, show.numbering.as_ref(), s.season.as_u32()))
            .collect();

        let names = self
            .db
            .xem_season_names(show_id, xem::origin(&show.remotes), targets)
            .await?;

        for (season, names) in seasons.iter_mut().zip(names) {
            season.alt_names = names;
        }

        Ok(())
    }

    pub(super) async fn find_episode_by_timestamp(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::FindEpisodeByTimestampRequest>()
            .context("Expected a request payload")?;
        let matched = self
            .db
            .find_episode_by_timestamp(req.show_id, req.timestamp)
            .await?;
        outgoing.write(api::FindEpisodeByTimestampResponse { matched });
        Ok(())
    }

    pub(super) async fn get_episode_releases(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::GetEpisodeReleasesRequest>()
            .context("Expected a request payload")?;
        let (releases, show_id, filters) = self.db.episode_release_rows(req.episode_id).await?;
        outgoing.write(api::GetEpisodeReleasesResponse {
            releases,
            show_id,
            filters,
        });
        Ok(())
    }

    pub(super) async fn get_episode_cache(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
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
        Ok(())
    }

    pub(super) async fn purge_episode_cache(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::PurgeEpisodeCacheRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_episode_cache(req.episode_id, req.source, None)
            .await?;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn sync_show(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SyncShowRequest>()
            .context("Expected a request payload")?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
            .await;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn sync_episode(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SyncEpisodeRequest>()
            .context("Expected a request payload")?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.show_id)
            .await?
            .context("Expected show to exist")?;

        let episode = self
            .db
            .episode_by_id(Some(self.user.id), req.episode_id)
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
        Ok(())
    }

    pub(super) async fn set_show_language(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetShowLanguageRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_show_language(self.user.id, req.id, req.language)
            .await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::ShowChanged { show: show.clone() },
            "ws set show language changed",
        );

        self.enqueue_show_sync(show.id, show.strings.title().map(str::to_owned), true)
            .await;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_show_include_specials(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetShowIncludeSpecialsRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_show_include_specials(self.user.id, req.id, req.include_specials)
            .await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::ShowChanged { show: show.clone() },
            "ws set show include specials changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_show_auto_sync(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetShowAutoSyncRequest>()
            .context("Expected a request payload")?;

        self.db.set_show_auto_sync(req.id, req.auto_sync).await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.broadcast.emit(
            incoming.channel(),
            api::AppEventKind::ShowChanged { show: show.clone() },
            "ws set show auto sync changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_show_numbering(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetShowNumberingRequest>()
            .context("Expected a request payload")?;

        let errors = req
            .numbering
            .as_ref()
            .map(api::Numbering::validate)
            .unwrap_or_default();

        if errors.is_empty() {
            self.db.set_show_numbering(req.id, req.numbering).await?;

            let show = self
                .db
                .show_by_id(Some(self.user.id), req.id)
                .await?
                .context("Expected show to exist")?;

            self.broadcast.emit(
                incoming.channel(),
                api::AppEventKind::ShowChanged { show },
                "ws set show numbering changed",
            );
        }

        outgoing.write(api::SetShowNumberingResponse { errors });
        Ok(())
    }

    pub(super) async fn get_show_numbering(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::GetShowNumberingRequest>()
            .context("Expected a request payload")?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        let config = self.db.load_config().await?;
        let (episodes, systems) = self.db.numbering_codes(req.id).await?;

        outgoing.write(api::ShowNumbering {
            tvdb_base: api::base_source(&show.remotes, &config) == Some(api::RemoteSource::Tvdb),
            episodes,
            systems,
        });

        Ok(())
    }

    pub(super) async fn set_show_air_date_filters(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
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
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.broadcast.emit(
            incoming.channel(),
            api::AppEventKind::ShowChanged { show: show.clone() },
            "ws set show air date filters changed",
        );

        for season in self.db.seasons(Some(self.user.id), req.id).await? {
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
        Ok(())
    }
}
