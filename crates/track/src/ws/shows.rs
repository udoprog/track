use anyhow::{Context as _, Result};
use api::ShowId;
use musli_web::ws;

use super::WsHandler;
use crate::db::Database;
use crate::remote::RemoteClients;
use crate::xem;

/// The show `remote` belongs to, also found by its counterpart on TMDB or
/// TheTVDB: a show tracked from one only gets the other's id once it syncs.
///
/// Remotes outlive a deleted show, so the id found may be one whose show is
/// gone. A show that still exists wins over one that doesn't; else the gone
/// show's id is returned so that tracking it again brings back its history.
pub(crate) async fn existing_show(
    db: &Database,
    remotes: &RemoteClients,
    remote: &api::Remote,
) -> Result<Option<ShowId>> {
    let direct = db.show_id_by_remote(remote).await?;

    if let Some(id) = direct
        && show_exists(db, id).await?
    {
        return Ok(Some(id));
    }

    let other = match remotes.cross_reference_show(remote).await {
        Ok(other) => other,
        Err(error) => {
            tracing::warn!(?remote, ?error, "Looking up the show's other remote");
            None
        }
    };

    let other = match other {
        Some(other) => db.show_id_by_remote(&other).await?,
        None => None,
    };

    if let Some(id) = other
        && show_exists(db, id).await?
    {
        return Ok(Some(id));
    }

    Ok(direct.or(other))
}

async fn show_exists(db: &Database, id: ShowId) -> Result<bool> {
    Ok(db.show_by_id(None, id).await?.is_some())
}

/// Tracks the show of `req` for `user`: the existing one if any (see
/// [`existing_show`]), created when its row doesn't exist. Returns the show
/// and whether it was created.
pub(crate) async fn track_show_for(
    db: &Database,
    remotes: &RemoteClients,
    user: api::UserId,
    req: &api::TrackShowRequest,
) -> Result<(api::Show, bool)> {
    let existing = existing_show(db, remotes, &req.remote).await?;

    let (show_id, created) = match existing {
        Some(id) if show_exists(db, id).await? => (id, false),
        Some(id) => (id, true),
        None => (ShowId::random(), true),
    };

    if created {
        db.create_show(
            show_id,
            &req.title
                .clone()
                .unwrap_or_else(|| req.remote.value().to_string()),
            None,
            req.overview.as_deref().unwrap_or_default(),
        )
        .await?;
    }

    db.add_remote(show_id, req.slug.as_deref(), &req.remote)
        .await?;

    db.set_show_tracked(user, show_id, true).await?;

    let show = db
        .show_by_id(Some(user), show_id)
        .await?
        .context("Expected show to exist")?;

    Ok((show, created))
}

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
        let pending = self.db.pending_code(self.user.id, req.show_id).await?;
        outgoing.write(api::ListSeasonsResponse { seasons, pending });
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

        let (show, created) = track_show_for(&self.db, &self.remote, self.user.id, &req).await?;

        if !created {
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
        let pending = self.db.pending_code(self.user.id, req.show_id).await?;
        outgoing.write(api::ListEpisodesResponse {
            episodes,
            watched,
            pending,
        });
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

    pub(super) async fn list_show_alt_names(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ListShowAltNamesRequest>()
            .context("Expected a request payload")?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        let system = xem::origin(&show.remotes);

        let names = self
            .db
            .xem_names(req.id)
            .await?
            .iter()
            .map(|n| api::ShowAltName {
                season: n.season.map(|season| api::LinkedSeason {
                    system: system.clone(),
                    season,
                }),
                name: n.to_api(),
            })
            .collect();

        outgoing.write(api::ShowAltNames { names });
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

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use anyhow::Result;
    use api::{Remote, ShowId};
    use axum::routing::get;

    use super::{existing_show, track_show_for};
    use crate::db::Database;
    use crate::remote::RemoteClients;

    /// Remote clients whose TMDB is a stand-in that knows Breaking Bad as
    /// TMDB 1396 and TheTVDB 81189.
    async fn remotes() -> Result<RemoteClients> {
        let app = axum::Router::new()
            .route(
                "/find/81189",
                get(|| async { axum::Json(serde_json::json!({ "tv_results": [{ "id": 1396 }] })) }),
            )
            .route(
                "/tv/1396/external_ids",
                get(|| async { axum::Json(serde_json::json!({ "id": 1396, "tvdb_id": 81189 })) }),
            );

        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await?;
        let addr = listener.local_addr()?;
        tokio::spawn(async move { axum::serve(listener, app).await });

        let http = reqwest::Client::new();
        let remotes = RemoteClients::new(http.clone(), http)
            .with_tmdb_base(reqwest::Url::parse(&format!("http://{addr}/"))?);
        remotes.configure(&api::Config {
            tmdb_api_key: "key".to_owned(),
            ..api::Config::default()
        })?;
        Ok(remotes)
    }

    async fn show_with(db: &Database, remote: Remote) -> Result<ShowId> {
        let show = ShowId::new(1);
        db.create_show(show, "Breaking Bad", None, "").await?;
        db.add_remote(show, None, &remote).await?;
        Ok(show)
    }

    /// A show tracked from TMDB whose sync hasn't found its TheTVDB id yet is
    /// still the show of its TheTVDB search result.
    #[tokio::test]
    async fn tvdb_result_finds_show_tracked_from_tmdb() -> Result<()> {
        let db = Database::open_scratch()?;
        let show = show_with(&db, Remote::tmdb(1396)).await?;
        let remotes = remotes().await?;

        let found = existing_show(&db, &remotes, &Remote::tvdb(81189)).await?;
        assert_eq!(found, Some(show));
        Ok(())
    }

    /// And the other way around.
    #[tokio::test]
    async fn tmdb_result_finds_show_tracked_from_tvdb() -> Result<()> {
        let db = Database::open_scratch()?;
        let show = show_with(&db, Remote::tvdb(81189)).await?;
        let remotes = remotes().await?;

        let found = existing_show(&db, &remotes, &Remote::tmdb(1396)).await?;
        assert_eq!(found, Some(show));
        Ok(())
    }

    /// A show TMDB doesn't know, or a failing lookup, is a new show.
    #[tokio::test]
    async fn unknown_show_is_new() -> Result<()> {
        let db = Database::open_scratch()?;
        show_with(&db, Remote::tmdb(1396)).await?;
        let remotes = remotes().await?;

        assert_eq!(existing_show(&db, &remotes, &Remote::tvdb(1)).await?, None);
        assert_eq!(existing_show(&db, &remotes, &Remote::tmdb(2)).await?, None);
        Ok(())
    }

    fn track(remote: Remote) -> api::TrackShowRequest {
        api::TrackShowRequest {
            slug: None,
            remote,
            title: Some("Breaking Bad".to_owned()),
            overview: None,
        }
    }

    /// Tracks a show from `tracked_from`, watches its first episode, deletes
    /// the show and tracks it again from `retracked_from`. The show comes back
    /// under its old id with its watch history once its episodes return.
    async fn retrack_deleted(tracked_from: Remote, retracked_from: Remote) -> Result<()> {
        let db = Database::open_scratch()?;
        let user = db.test_root().await?;
        let remotes = remotes().await?;

        let (show, created) = track_show_for(&db, &remotes, user, &track(tracked_from)).await?;
        assert!(created);

        let season = api::SeasonNumber::from_ordinal(1);
        let episode = api::EpisodeId::new(11);
        let aired = api::Timestamp::from_jiff(jiff::Timestamp::from_second(1_600_000_000)?);
        let now = api::Timestamp::from_jiff(jiff::Timestamp::from_second(1_700_000_000)?);
        db.upsert_episode(episode, show.id, season, 1, None, Some(aired))
            .await?;
        db.mark_watched(
            user,
            api::WatchedId::random(),
            api::WatchedKind::Episode {
                show: show.id,
                episode,
            },
            api::MarkTime::Now,
            now,
        )
        .await?;

        db.delete_show(show.id).await?;
        assert!(db.show_by_id(None, show.id).await?.is_none());

        let (again, created) = track_show_for(&db, &remotes, user, &track(retracked_from)).await?;
        assert!(created);
        assert_eq!(again.id, show.id);
        assert!(again.tracked);

        // Its sync brings the episodes back, and the watch is still there.
        db.upsert_episode(episode, show.id, season, 1, None, Some(aired))
            .await?;
        assert_eq!(db.episodes_watched(user, show.id).await?.len(), 1);
        Ok(())
    }

    /// A deleted show tracked again from the same remote keeps its id.
    #[tokio::test]
    async fn deleted_show_tracked_again_keeps_its_id() -> Result<()> {
        retrack_deleted(Remote::tmdb(1396), Remote::tmdb(1396)).await
    }

    /// Also when tracked again from its counterpart's search result.
    #[tokio::test]
    async fn deleted_show_tracked_again_from_its_counterpart_keeps_its_id() -> Result<()> {
        retrack_deleted(Remote::tmdb(1396), Remote::tvdb(81189)).await
    }

    /// A show that still exists wins over a deleted one that shares the
    /// show's other remote.
    #[tokio::test]
    async fn live_show_wins_over_a_deleted_one() -> Result<()> {
        let db = Database::open_scratch()?;
        let remotes = remotes().await?;

        let gone = ShowId::new(1);
        db.create_show(gone, "Breaking Bad", None, "").await?;
        db.add_remote(gone, None, &Remote::tvdb(81189)).await?;
        db.delete_show(gone).await?;

        let live = ShowId::new(2);
        db.create_show(live, "Breaking Bad", None, "").await?;
        db.add_remote(live, None, &Remote::tmdb(1396)).await?;

        let found = existing_show(&db, &remotes, &Remote::tvdb(81189)).await?;
        assert_eq!(found, Some(live));
        Ok(())
    }
}
