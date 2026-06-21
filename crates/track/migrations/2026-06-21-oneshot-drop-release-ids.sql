-- Drop the unused surrogate `id` from the release tables. Both `movie_releases`
-- and `episode_releases` are only ever accessed through their natural UNIQUE
-- keys, so the integer primary key was dead weight. SQLite cannot drop a PRIMARY
-- KEY column in place, so the tables are rebuilt with the natural key promoted to
-- the PRIMARY KEY. Existing rows are preserved (the old UNIQUE constraint already
-- guarantees no key collisions).
CREATE TABLE
    movie_releases_new (
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        country INTEGER NOT NULL DEFAULT 0,
        release_type INTEGER NOT NULL,
        timestamp INTEGER NOT NULL,
        PRIMARY KEY (movie_id, country, release_type)
    );

INSERT INTO
    movie_releases_new (movie_id, country, release_type, timestamp)
SELECT
    movie_id, country, release_type, timestamp
FROM
    movie_releases;

DROP TABLE movie_releases;

ALTER TABLE movie_releases_new RENAME TO movie_releases;

CREATE INDEX idx_movie_releases_movie ON movie_releases (movie_id, release_type, timestamp);

-- The natural primary key leads with `episode_id`, so it already serves the
-- show-join lookup the old standalone index covered; that index is not recreated.
CREATE TABLE
    episode_releases_new (
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        source INTEGER NOT NULL,
        country INTEGER NOT NULL DEFAULT 0,
        network TEXT NOT NULL DEFAULT '',
        timestamp INTEGER NOT NULL,
        PRIMARY KEY (episode_id, source, country, network)
    );

INSERT INTO
    episode_releases_new (episode_id, source, country, network, timestamp)
SELECT
    episode_id, source, country, network, timestamp
FROM
    episode_releases;

DROP TABLE episode_releases;

ALTER TABLE episode_releases_new RENAME TO episode_releases;
