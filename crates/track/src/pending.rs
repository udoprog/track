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

    pub(crate) async fn fill_for_series(
        &self,
        series_id: api::SeriesId,
        now: api::Timestamp,
    ) -> Result<()> {
        self.db.fill_pending_for_series(series_id, now).await
    }

    pub(crate) async fn on_episode_watched_from(
        &self,
        series_id: api::SeriesId,
        episode_id: api::EpisodeId,
        now: api::Timestamp,
    ) -> Result<()> {
        self.db.remove_pending_episode(series_id).await?;
        self.db
            .fill_pending_for_series_from(series_id, episode_id, now)
            .await
    }
}
