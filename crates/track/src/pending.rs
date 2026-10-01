use anyhow::Result;

use crate::db::Database;

#[derive(Clone)]
pub(crate) struct PendingSystem {
    db: Database,
}

impl PendingSystem {
    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }

    pub(crate) async fn fill_for_show(
        &self,
        show_id: api::ShowId,
        include_specials: bool,
        now: api::Timestamp,
    ) -> Result<()> {
        self.db
            .fill_pending_for_show(show_id, include_specials, now)
            .await
    }

    pub(crate) async fn on_episode_watched_from(
        &self,
        user: api::UserId,
        show_id: api::ShowId,
        episode_id: api::EpisodeId,
        now: api::Timestamp,
    ) -> Result<()> {
        self.db.remove_pending_episode(user, show_id).await?;
        self.db
            .fill_pending_for_show_from(user, show_id, episode_id, now)
            .await
    }
}
