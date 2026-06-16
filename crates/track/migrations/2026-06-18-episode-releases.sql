-- Known episode air dates move into their own table, attributed to the remote
-- source (and optionally country/network) they came from. The effective
-- `episodes.aired` is recomputed from these by priority (see api::effective_aired)
-- and kept as a denormalized column so existing schedule/pending queries are
-- unaffected. Mirrors the movies.release_date + movie_releases pattern.
CREATE TABLE episode_releases (
    id INTEGER PRIMARY KEY,
    episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
    source INTEGER NOT NULL,                 -- api::RemoteSource (0 = Unknown)
    country TEXT NOT NULL DEFAULT '',
    network TEXT NOT NULL DEFAULT '',
    timestamp INTEGER NOT NULL,
    UNIQUE (episode_id, source, country, network)
);

CREATE INDEX idx_episode_releases_episode ON episode_releases (episode_id);

-- Backfill existing air dates as Unknown-source so nothing is lost; rowids are
-- assigned automatically (these ids are not referenced elsewhere).
INSERT INTO episode_releases (episode_id, source, country, network, timestamp)
    SELECT id, 0, '', '', aired FROM episodes WHERE aired IS NOT NULL;

-- Per-show override of which air dates qualify; NULL = global default (config).
ALTER TABLE shows ADD COLUMN air_date_filters TEXT;
