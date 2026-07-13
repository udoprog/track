-- Per-episode air-window sync: episodes get their own sync timestamp so the
-- background poller can re-sync them hourly around their air date, and a per-source
-- cache slot so those calls can be deduplicated by the remote.
--
-- `last_synced_at` is nullable with no default, so a plain ADD COLUMN suffices - no
-- table rebuild. NULL means "never episode-synced", which sorts first in the
-- air-window query.
ALTER TABLE episodes
ADD COLUMN last_synced_at INTEGER;

CREATE INDEX idx_episodes_air_sync ON episodes (aired, last_synced_at)
WHERE
    aired IS NOT NULL;

CREATE TABLE
    episode_cache (
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        source INTEGER NOT NULL,
        cache TEXT NOT NULL,
        PRIMARY KEY (episode_id, source)
    );
