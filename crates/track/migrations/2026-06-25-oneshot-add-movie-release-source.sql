-- Add `source` to `movie_releases` and promote it into the natural primary key,
-- mirroring `episode_releases`. SQLite cannot add a column into an existing
-- PRIMARY KEY in place, so the table is rebuilt. Every existing row was synced
-- from TMDB (the only movie release source), so they are backfilled with the
-- TMDB source value (2).
CREATE TABLE
    movie_releases_new (
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        source INTEGER NOT NULL DEFAULT 2,
        country INTEGER NOT NULL DEFAULT 0,
        release_type INTEGER NOT NULL,
        timestamp INTEGER NOT NULL,
        PRIMARY KEY (movie_id, source, country, release_type)
    );

INSERT INTO
    movie_releases_new (movie_id, source, country, release_type, timestamp)
SELECT
    movie_id, 2, country, release_type, timestamp
FROM
    movie_releases;

DROP TABLE movie_releases;

ALTER TABLE movie_releases_new RENAME TO movie_releases;

CREATE INDEX idx_movie_releases_movie ON movie_releases (movie_id, release_type, timestamp);
