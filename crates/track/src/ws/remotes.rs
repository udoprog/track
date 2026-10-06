use anyhow::{Context as _, Result};
use musli_web::api::ChannelId;
use musli_web::ws;

use crate::db::RemoteOwner;

use super::WsHandler;

/// A change to one owner's remotes, read from its per-owner request.
enum Edit {
    Add {
        slug: Option<String>,
        remote: api::Remote,
    },
    Remove {
        remote_id: api::RemoteId,
    },
    Update {
        remote_id: api::RemoteId,
        slug: Option<String>,
        remote: api::Remote,
    },
    SetEnabled {
        remote_id: api::RemoteId,
        enabled: bool,
    },
    SetSyncKinds {
        remote_id: api::RemoteId,
        sync_kinds: Option<api::SyncKindSet>,
    },
    Reorder {
        remote_ids: Vec<api::RemoteId>,
    },
    PurgeCache {
        remote_id: api::RemoteId,
    },
}

trait Owner: RemoteOwner {
    /// Shows and movies must exist before a remote is added, removed, updated
    /// or purged, and adding, removing or updating one changes the pending list.
    const MEDIA: bool;

    /// The owner's changed event and its title.
    async fn load(self, h: &WsHandler) -> Result<(api::AppEventKind, Option<String>)>;

    async fn enqueue_sync(self, h: &WsHandler, title: Option<String>);

    /// Clear validators the remote also holds below the owner.
    async fn purge_nested_cache(self, _: &WsHandler, _: api::RemoteId) -> Result<()> {
        Ok(())
    }
}

impl Owner for api::ShowId {
    const MEDIA: bool = true;

    async fn load(self, h: &WsHandler) -> Result<(api::AppEventKind, Option<String>)> {
        let show =
            h.db.show_by_id(Some(h.user.id), self)
                .await?
                .context("Expected show to exist")?;

        let title = show.strings.title().map(str::to_owned);
        Ok((api::AppEventKind::ShowChanged { show }, title))
    }

    async fn enqueue_sync(self, h: &WsHandler, title: Option<String>) {
        h.enqueue_show_sync(self, title, true).await;
    }

    // The remote also holds a validator on each of the show's episodes;
    // leaving those behind would let a "force resync" still be answered from
    // cache at the episode level.
    async fn purge_nested_cache(self, h: &WsHandler, remote_id: api::RemoteId) -> Result<()> {
        let show =
            h.db.show_by_id(Some(h.user.id), self)
                .await?
                .context("Expected show to exist")?;

        if let Some(entry) = show.remotes.iter().find(|e| e.id == remote_id) {
            h.db.clear_episode_cache_for_show_source(self, *entry.remote.source())
                .await?;
        }

        Ok(())
    }
}

impl Owner for api::MovieId {
    const MEDIA: bool = true;

    async fn load(self, h: &WsHandler) -> Result<(api::AppEventKind, Option<String>)> {
        let movie =
            h.db.movie_by_id(Some(h.user.id), self)
                .await?
                .context("Expected movie to exist")?;

        let title = movie.strings.title().map(str::to_owned);
        Ok((api::AppEventKind::MovieChanged { movie }, title))
    }

    async fn enqueue_sync(self, h: &WsHandler, title: Option<String>) {
        h.enqueue_movie_sync(self, title, true).await;
    }
}

impl Owner for api::PersonId {
    const MEDIA: bool = false;

    async fn load(self, h: &WsHandler) -> Result<(api::AppEventKind, Option<String>)> {
        let person =
            h.db.person_by_id(None, self)
                .await?
                .context("Expected person to exist")?;

        let title = person.name.title().map(str::to_owned);
        Ok((api::AppEventKind::PersonChanged { person_id: self }, title))
    }

    async fn enqueue_sync(self, h: &WsHandler, title: Option<String>) {
        h.enqueue_person_sync(self, title, true).await;
    }
}

macro_rules! handlers {
    ($($name:ident($request:ty) => |$req:ident| $edit:expr;)*) => {
        $(
            pub(super) async fn $name(
                &self,
                incoming: &mut ws::Incoming<'_>,
                outgoing: &mut ws::Outgoing<'_>,
            ) -> Result<()> {
                let $req = incoming
                    .read::<$request>()
                    .context("Expected a request payload")?;

                self.edit_remote(incoming.channel(), $req.id, $edit).await?;
                outgoing.write(api::Empty);
                Ok(())
            }
        )*
    };
}

impl WsHandler {
    handlers! {
        add_show_remote(api::AddShowRemoteRequest) => |req| Edit::Add { slug: req.slug, remote: req.remote };
        remove_show_remote(api::RemoveShowRemoteRequest) => |req| Edit::Remove { remote_id: req.remote_id };
        update_show_remote(api::UpdateShowRemoteRequest) => |req| Edit::Update { remote_id: req.remote_id, slug: req.slug, remote: req.remote };
        set_show_remote_enabled(api::SetShowRemoteEnabledRequest) => |req| Edit::SetEnabled { remote_id: req.remote_id, enabled: req.enabled };
        set_show_remote_sync_kinds(api::SetShowRemoteSyncKindsRequest) => |req| Edit::SetSyncKinds { remote_id: req.remote_id, sync_kinds: req.sync_kinds };
        reorder_show_remotes(api::ReorderShowRemotesRequest) => |req| Edit::Reorder { remote_ids: req.remote_ids };
        purge_show_remote_cache(api::PurgeShowRemoteCacheRequest) => |req| Edit::PurgeCache { remote_id: req.remote_id };

        add_movie_remote(api::AddMovieRemoteRequest) => |req| Edit::Add { slug: req.slug, remote: req.remote };
        remove_movie_remote(api::RemoveMovieRemoteRequest) => |req| Edit::Remove { remote_id: req.remote_id };
        update_movie_remote(api::UpdateMovieRemoteRequest) => |req| Edit::Update { remote_id: req.remote_id, slug: req.slug, remote: req.remote };
        set_movie_remote_enabled(api::SetMovieRemoteEnabledRequest) => |req| Edit::SetEnabled { remote_id: req.remote_id, enabled: req.enabled };
        set_movie_remote_sync_kinds(api::SetMovieRemoteSyncKindsRequest) => |req| Edit::SetSyncKinds { remote_id: req.remote_id, sync_kinds: req.sync_kinds };
        reorder_movie_remotes(api::ReorderMovieRemotesRequest) => |req| Edit::Reorder { remote_ids: req.remote_ids };
        purge_movie_remote_cache(api::PurgeMovieRemoteCacheRequest) => |req| Edit::PurgeCache { remote_id: req.remote_id };

        add_person_remote(api::AddPersonRemoteRequest) => |req| Edit::Add { slug: req.slug, remote: req.remote };
        remove_person_remote(api::RemovePersonRemoteRequest) => |req| Edit::Remove { remote_id: req.remote_id };
        update_person_remote(api::UpdatePersonRemoteRequest) => |req| Edit::Update { remote_id: req.remote_id, slug: req.slug, remote: req.remote };
        set_person_remote_enabled(api::SetPersonRemoteEnabledRequest) => |req| Edit::SetEnabled { remote_id: req.remote_id, enabled: req.enabled };
        set_person_remote_sync_kinds(api::SetPersonRemoteSyncKindsRequest) => |req| Edit::SetSyncKinds { remote_id: req.remote_id, sync_kinds: req.sync_kinds };
        reorder_person_remotes(api::ReorderPersonRemotesRequest) => |req| Edit::Reorder { remote_ids: req.remote_ids };
        purge_person_remote_cache(api::PurgePersonRemoteCacheRequest) => |req| Edit::PurgeCache { remote_id: req.remote_id };
    }

    /// Apply `edit`, broadcast the owner, then either resync it or (for shows
    /// and movies) announce a pending-list change.
    async fn edit_remote<O: Owner>(&self, channel: ChannelId, owner: O, edit: Edit) -> Result<()> {
        if O::MEDIA
            && !matches!(
                edit,
                Edit::SetEnabled { .. } | Edit::SetSyncKinds { .. } | Edit::Reorder { .. }
            )
        {
            owner.load(self).await?;
        }

        let db = &self.db;

        let resync = match edit {
            Edit::Add { slug, remote } => {
                db.add_remote(owner, slug.as_deref(), &remote).await?;
                false
            }
            Edit::Remove { remote_id } => {
                db.remove_remote(owner, remote_id).await?;
                false
            }
            Edit::Update {
                remote_id,
                slug,
                remote,
            } => {
                db.update_remote(owner, remote_id, slug.as_deref(), &remote)
                    .await?;
                false
            }
            Edit::SetEnabled { remote_id, enabled } => {
                db.set_remote_enabled(owner, remote_id, enabled).await?;
                true
            }
            Edit::SetSyncKinds {
                remote_id,
                sync_kinds,
            } => {
                db.set_remote_sync_kinds(owner, remote_id, sync_kinds)
                    .await?;
                true
            }
            Edit::Reorder { remote_ids } => {
                db.reorder_remotes(owner, remote_ids).await?;
                true
            }
            Edit::PurgeCache { remote_id } => {
                db.set_remote_cache(owner, remote_id, None).await?;
                owner.purge_nested_cache(self, remote_id).await?;
                true
            }
        };

        let (event, title) = owner.load(self).await?;
        self.broadcast.emit(channel, event, "ws remote changed");

        if resync {
            owner.enqueue_sync(self, title).await;
        } else if O::MEDIA {
            self.broadcast.emit(
                channel,
                api::AppEventKind::PendingChanged,
                "ws remote pending changed",
            );
        }

        Ok(())
    }
}
