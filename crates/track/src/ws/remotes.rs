use anyhow::{Context as _, Result};
use musli_web::ws;

use super::WsHandler;

impl WsHandler {
    pub(super) async fn add_show_remote(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::AddShowRemoteRequest>()
            .context("Expected a request payload")?;

        self.db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.db
            .add_show_remote(req.id, req.slug.as_deref(), &req.remote)
            .await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
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
        Ok(())
    }

    pub(super) async fn remove_show_remote(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::RemoveShowRemoteRequest>()
            .context("Expected a request payload")?;

        self.db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.db.remove_show_remote(req.id, req.remote_id).await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
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
        Ok(())
    }

    pub(super) async fn update_show_remote(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::UpdateShowRemoteRequest>()
            .context("Expected a request payload")?;

        self.db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.db
            .update_show_remote(req.id, req.remote_id, req.slug.as_deref(), &req.remote)
            .await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
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
        Ok(())
    }

    pub(super) async fn set_show_remote_enabled(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetShowRemoteEnabledRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_show_remote_enabled(req.id, req.remote_id, req.enabled)
            .await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
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
        Ok(())
    }

    pub(super) async fn set_show_remote_sync_kinds(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetShowRemoteSyncKindsRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_show_remote_sync_kinds(req.id, req.remote_id, req.sync_kinds)
            .await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
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
        Ok(())
    }

    pub(super) async fn reorder_show_remotes(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ReorderShowRemotesRequest>()
            .context("Expected a request payload")?;

        self.db.reorder_show_remotes(req.id, req.remote_ids).await?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
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
        Ok(())
    }

    pub(super) async fn purge_show_remote_cache(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::PurgeShowRemoteCacheRequest>()
            .context("Expected a request payload")?;

        let show = self
            .db
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.db
            .set_show_remote_cache(req.id, req.remote_id, None)
            .await?;

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
            .show_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected show to exist")?;

        self.broadcast.emit(
            incoming.channel(),
            api::AppEventKind::ShowChanged { show },
            "ws purge show remote cache",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn add_movie_remote(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::AddMovieRemoteRequest>()
            .context("Expected a request payload")?;

        self.db
            .movie_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected movie to exist")?;

        self.db
            .add_movie_remote(req.id, req.slug.as_deref(), &req.remote)
            .await?;

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
            "ws add movie remote changed",
        );

        self.broadcast.emit(
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws add movie remote pending changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn remove_movie_remote(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::RemoveMovieRemoteRequest>()
            .context("Expected a request payload")?;

        self.db
            .movie_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected movie to exist")?;

        self.db.remove_movie_remote(req.id, req.remote_id).await?;

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
            "ws remove movie remote changed",
        );

        self.broadcast.emit(
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws remove movie remote pending changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn update_movie_remote(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::UpdateMovieRemoteRequest>()
            .context("Expected a request payload")?;

        self.db
            .movie_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected movie to exist")?;

        self.db
            .update_movie_remote(req.id, req.remote_id, req.slug.as_deref(), &req.remote)
            .await?;

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
            "ws update movie remote changed",
        );

        self.broadcast.emit(
            incoming.channel(),
            api::AppEventKind::PendingChanged,
            "ws update movie remote pending changed",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_movie_remote_enabled(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetMovieRemoteEnabledRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_movie_remote_enabled(req.id, req.remote_id, req.enabled)
            .await?;

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
            "ws set movie remote enabled changed",
        );

        self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
            .await;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_movie_remote_sync_kinds(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetMovieRemoteSyncKindsRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_movie_remote_sync_kinds(req.id, req.remote_id, req.sync_kinds)
            .await?;

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
            "ws set movie remote sync kinds changed",
        );

        self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
            .await;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn reorder_movie_remotes(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ReorderMovieRemotesRequest>()
            .context("Expected a request payload")?;

        self.db
            .reorder_movie_remotes(req.id, req.remote_ids)
            .await?;

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
            "ws reorder movie remotes changed",
        );

        self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
            .await;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn purge_movie_remote_cache(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::PurgeMovieRemoteCacheRequest>()
            .context("Expected a request payload")?;

        let movie = self
            .db
            .movie_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected movie to exist")?;

        self.db
            .set_movie_remote_cache(req.id, req.remote_id, None)
            .await?;

        self.enqueue_movie_sync(movie.id, movie.strings.title().map(str::to_owned), true)
            .await;

        let movie = self
            .db
            .movie_by_id(Some(self.user.id), req.id)
            .await?
            .context("Expected movie to exist")?;

        self.broadcast.emit(
            incoming.channel(),
            api::AppEventKind::MovieChanged { movie },
            "ws purge movie remote cache",
        );

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn add_person_remote(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::AddPersonRemoteRequest>()
            .context("Expected a request payload")?;

        self.db
            .add_person_remote(req.id, req.slug.as_deref(), &req.remote)
            .await?;

        self.broadcast_person_changed(incoming.channel(), req.id, "ws add person remote")
            .await?;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn remove_person_remote(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::RemovePersonRemoteRequest>()
            .context("Expected a request payload")?;

        self.db.remove_person_remote(req.id, req.remote_id).await?;

        self.broadcast_person_changed(incoming.channel(), req.id, "ws remove person remote")
            .await?;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn update_person_remote(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::UpdatePersonRemoteRequest>()
            .context("Expected a request payload")?;

        self.db
            .update_person_remote(req.id, req.remote_id, req.slug.as_deref(), &req.remote)
            .await?;

        self.broadcast_person_changed(incoming.channel(), req.id, "ws update person remote")
            .await?;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_person_remote_enabled(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetPersonRemoteEnabledRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_person_remote_enabled(req.id, req.remote_id, req.enabled)
            .await?;

        self.resync_person(incoming.channel(), req.id, "ws set person remote enabled")
            .await?;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn set_person_remote_sync_kinds(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::SetPersonRemoteSyncKindsRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_person_remote_sync_kinds(req.id, req.remote_id, req.sync_kinds)
            .await?;

        self.resync_person(
            incoming.channel(),
            req.id,
            "ws set person remote sync kinds",
        )
        .await?;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn reorder_person_remotes(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::ReorderPersonRemotesRequest>()
            .context("Expected a request payload")?;

        self.db
            .reorder_person_remotes(req.id, req.remote_ids)
            .await?;

        self.resync_person(incoming.channel(), req.id, "ws reorder person remotes")
            .await?;

        outgoing.write(api::Empty);
        Ok(())
    }

    pub(super) async fn purge_person_remote_cache(
        &self,
        incoming: &mut ws::Incoming<'_>,
        outgoing: &mut ws::Outgoing<'_>,
    ) -> Result<()> {
        let req = incoming
            .read::<api::PurgePersonRemoteCacheRequest>()
            .context("Expected a request payload")?;

        self.db
            .set_person_remote_cache(req.id, req.remote_id, None)
            .await?;

        self.resync_person(incoming.channel(), req.id, "ws purge person remote cache")
            .await?;

        outgoing.write(api::Empty);
        Ok(())
    }
}
