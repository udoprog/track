use anyhow::{Context as _, Result};
use musli_web::ws;

use super::WsHandler;

impl WsHandler {
    pub(super) async fn get_system_config(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let _req = incoming
            .read::<api::GetSystemConfigRequest>()
            .context("Expected a request payload")?;

        let config = self.db.load_config().await?;

        outgoing.write(api::GetSystemConfigResponse { config });
        Ok(())
    }

    pub(super) async fn set_system_config(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetSystemConfigRequest>()
            .context("Expected a request payload")?;

        let prev = self.db.load_config().await?;

        self.db.save_config(&req.config).await?;
        self.remote.configure(&req.config)?;
        self.auth.configure(&req.config);

        self.config_changed.notify_one();

        self.broadcast.emit_to_admins(
            incoming.channel(),
            api::AppEventKind::ConfigChanged {
                config: req.config.clone(),
            },
        );

        let site = req.config.site();

        if prev.site() != site {
            self.broadcast.emit(
                incoming.channel(),
                api::AppEventKind::SiteConfigChanged { site },
                "ws site config changed",
            );
        }

        // The global air-date filters feed every show's effective dates;
        // recompute when they change (per-show overrides use their own).
        if prev.air_date_filters != req.config.air_date_filters {
            let default = req.config.air_date_filters.clone();

            for show in self.db.shows(None).await? {
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

            for movie in self.db.movies(None).await? {
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
        Ok(())
    }

    pub(super) async fn get_preferences(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let _req = incoming
            .read::<api::GetPreferencesRequest>()
            .context("Expected a request payload")?;

        let preferences = self.db.load_preferences(self.user.id).await?;
        let site = self.db.load_config().await?.site();

        outgoing.write(api::GetPreferencesResponse { preferences, site });
        Ok(())
    }

    pub(super) async fn set_preferences(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetPreferencesRequest>()
            .context("Expected a request payload")?;

        self.db
            .save_preferences(self.user.id, &req.preferences)
            .await?;

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PreferencesChanged {
                preferences: req.preferences,
            },
            "ws preferences changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn get_top_languages(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let _req = incoming
            .read::<api::GetTopLanguagesRequest>()
            .context("Expected a request payload")?;

        let top_languages = self.db.get_state_top_languages().await?;

        outgoing.write(api::GetTopLanguagesResponse { top_languages });
        Ok(())
    }
}
