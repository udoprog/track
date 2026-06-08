use anyhow::Result;
use db::Database;

#[derive(Clone)]
pub(crate) struct PendingSystem {
    db: Database,
}

impl PendingSystem {
    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }

    /// Fill the pending slot for a series only if it is currently empty.
    /// Called after sync upserts a series' episodes.
    pub(crate) async fn fill_for_series(&self, series_id: api::SeriesId) -> Result<()> {
        self.db.fill_pending_for_series(series_id).await
    }

    /// Remove the watched episode from pending, then fill the now-empty slot.
    /// Called by the MarkWatched handler for episode watches.
    pub(crate) async fn on_episode_watched(&self, series_id: api::SeriesId) -> Result<()> {
        self.db.remove_pending_episode(series_id).await?;
        self.db.fill_pending_for_series(series_id).await
    }
}
