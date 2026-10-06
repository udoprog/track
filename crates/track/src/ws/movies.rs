use anyhow::{Context as _, Result};
use api::MovieId;
use musli_web::ws;

use super::WsHandler;

impl WsHandler {
    pub(super) async fn get_movie_releases(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::GetMovieReleasesRequest>()
            .context("Expected a request payload")?;
        let (releases, filters) = self.db.movie_release_rows(req.movie_id).await?;
        outgoing.write(api::GetMovieReleasesResponse { releases, filters });
        Ok(())
    }

    pub(super) async fn get_movie(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::GetMovieRequest>()
            .context("Expected a request payload")?;
        let movie = self.db.movie_by_id(Some(self.user.id), req.id).await?;
        outgoing.write(movie);
        Ok(())
    }

    pub(super) async fn track_movie(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::TrackMovieRequest>()
            .context("Expected a request payload")?;

        let existing = self.db.movie_id_by_remote(&req.remote).await?;
        let movie_id = existing.unwrap_or_else(MovieId::random);

        if existing.is_none() {
            self.db
                .create_movie(movie_id, &req.remote.value().to_string(), None, "")
                .await?;
        }

        self.db
            .add_remote(movie_id, req.slug.as_deref(), &req.remote)
            .await?;

        self.db
            .set_movie_tracked(self.user.id, movie_id, true)
            .await?;

        if existing.is_some() {
            let config = self.db.load_config().await?;
            self.db
                .update_movie_pending(movie_id, config.release_filters)
                .await?;
        }

        let movie = self
            .db
            .movie_by_id(Some(self.user.id), movie_id)
            .await?
            .context("Expected movie to exist")?;

        let created = api::AppEventKind::MovieCreated {
            movie: movie.clone(),
        };

        if existing.is_some() {
            self.broadcast.emit_to(
                self.user.id,
                incoming.channel(),
                created,
                "ws track existing movie",
            );
        } else {
            self.broadcast
                .emit(incoming.channel(), created, "ws track movie created");
        }

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws track movie pending changed",
        );

        self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
            .await;

        outgoing.write(movie);
        Ok(())
    }

    pub(super) async fn untrack_movie(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::UntrackMovieRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_movie_tracked(self.user.id, req.id, req.tracked)
            .await?;

        if req.tracked {
            let config = self.db.load_config().await?;
            self.db
                .update_movie_pending(req.id, config.release_filters)
                .await?;
        }

        let movie = self
            .db
            .movie_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected movie to exist")?;

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::MovieChanged {
                movie: movie.clone(),
            },
            "ws untrack movie changed",
        );

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws untrack movie pending changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn remove_movie(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
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
        Ok(())
    }

    pub(super) async fn sync_movie(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SyncMovieRequest>()
            .context("Expected a request payload")?;

        let movie = self
            .db
            .movie_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected movie to exist")?;

        self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
            .await;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_movie_language(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetMovieLanguageRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_movie_language(self.user.id, req.id, req.language)
            .await?;

        let movie = self
            .db
            .movie_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected movie to exist")?;

        self.broadcast.emit_to(
            self.user.id,
            incoming.channel(),
            api::AppEventKind::MovieChanged {
                movie: movie.clone(),
            },
            "ws set movie language changed",
        );

        self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
            .await;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_movie_auto_sync(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetMovieAutoSyncRequest>()
            .context("Expected a request payload")?;

        self.db.set_movie_auto_sync(req.id, req.auto_sync).await?;

        let movie = self
            .db
            .movie_by_id(Some(self.user.id), req.id)
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
        Ok(())
    }

    pub(super) async fn set_movie_release_filters(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
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
            .movie_by_id(Some(self.user.id), req.id)
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
        Ok(())
    }
}
