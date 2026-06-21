-- Change the country columns from TEXT to an integer enum. SQLite cannot alter a
-- column's type in place, so the affected tables are rebuilt. Existing rows are
-- preserved, but the country value itself is reset to 0; collapsing it can cause
-- rows that previously differed only by country to collide on the UNIQUE
-- constraint, so duplicates are dropped via INSERT OR IGNORE.
CREATE TABLE
    movie_releases_new (
        id INTEGER PRIMARY KEY,
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        country INTEGER NOT NULL DEFAULT 0,
        release_type INTEGER NOT NULL,
        timestamp INTEGER NOT NULL,
        UNIQUE (movie_id, country, release_type)
    );

INSERT OR IGNORE INTO
    movie_releases_new (id, movie_id, country, release_type, timestamp)
SELECT
    id, movie_id, 0, release_type, timestamp
FROM
    movie_releases;

DROP TABLE movie_releases;

ALTER TABLE movie_releases_new RENAME TO movie_releases;

CREATE INDEX idx_movie_releases_movie ON movie_releases (movie_id, release_type, timestamp);

CREATE TABLE
    episode_releases_new (
        id INTEGER PRIMARY KEY,
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        source INTEGER NOT NULL,
        country INTEGER NOT NULL DEFAULT 0,
        network TEXT NOT NULL DEFAULT '',
        timestamp INTEGER NOT NULL,
        UNIQUE (episode_id, source, country, network)
    );

INSERT OR IGNORE INTO
    episode_releases_new (id, episode_id, source, country, network, timestamp)
SELECT
    id, episode_id, source, 0, network, timestamp
FROM
    episode_releases;

DROP TABLE episode_releases;

ALTER TABLE episode_releases_new RENAME TO episode_releases;

CREATE INDEX idx_episode_releases_episode ON episode_releases (episode_id);

-- The country reset above wiped source data that the background recompute paths
-- (movie pending, episode air dates) depend on. Clear last_synced_at so the next
-- background poll treats every show/movie as stale and re-fetches it, repopulating
-- real country values before any recompute runs against the wiped releases.
UPDATE shows SET last_synced_at = NULL;

UPDATE movies SET last_synced_at = NULL;
