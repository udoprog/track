-- Tracking, watch history and pending become per user. Everything recorded so
-- far belongs to root (the first administrator if root was renamed).
CREATE TEMP TABLE migration_owner AS
SELECT
    id
FROM
    users
WHERE
    role = 'admin'
ORDER BY
    login <> 'root',
    id
LIMIT
    1;

CREATE TABLE
    user_tracked_shows (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        PRIMARY KEY (user_id, show_id)
    );

CREATE INDEX idx_user_tracked_shows_show ON user_tracked_shows (show_id);

INSERT INTO
    user_tracked_shows (user_id, show_id)
SELECT
    (SELECT id FROM migration_owner),
    id
FROM
    shows
WHERE
    tracked = 1;

CREATE TABLE
    user_tracked_movies (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        PRIMARY KEY (user_id, movie_id)
    );

CREATE INDEX idx_user_tracked_movies_movie ON user_tracked_movies (movie_id);

INSERT INTO
    user_tracked_movies (user_id, movie_id)
SELECT
    (SELECT id FROM migration_owner),
    id
FROM
    movies
WHERE
    tracked = 1;

ALTER TABLE shows
DROP COLUMN tracked;

ALTER TABLE movies
DROP COLUMN tracked;

CREATE TABLE
    watched_episodes_new (
        id INTEGER PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        timestamp INTEGER NOT NULL,
        show_id INTEGER,
        season INTEGER NOT NULL,
        episode INTEGER NOT NULL
    );

INSERT INTO
    watched_episodes_new (id, user_id, timestamp, show_id, season, episode)
SELECT
    id,
    (SELECT id FROM migration_owner),
    timestamp,
    show_id,
    season,
    episode
FROM
    watched_episodes;

DROP TABLE watched_episodes;

ALTER TABLE watched_episodes_new
RENAME TO watched_episodes;

CREATE INDEX idx_watched_episodes_show ON watched_episodes (user_id, show_id, season, episode);

CREATE TABLE
    watched_movies_new (
        id INTEGER PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        timestamp INTEGER NOT NULL,
        movie_id INTEGER
    );

INSERT INTO
    watched_movies_new (id, user_id, timestamp, movie_id)
SELECT
    id,
    (SELECT id FROM migration_owner),
    timestamp,
    movie_id
FROM
    watched_movies;

DROP TABLE watched_movies;

ALTER TABLE watched_movies_new
RENAME TO watched_movies;

CREATE INDEX idx_watched_movies_movie ON watched_movies (user_id, movie_id);

CREATE TABLE
    pending_new (
        id INTEGER PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        timestamp INTEGER NOT NULL,
        show_id INTEGER REFERENCES shows (id) ON DELETE CASCADE,
        episode_id INTEGER REFERENCES episodes (id) ON DELETE CASCADE,
        movie_id INTEGER REFERENCES movies (id) ON DELETE CASCADE,
        CHECK (
            (
                show_id IS NOT NULL
                AND episode_id IS NOT NULL
            )
            OR (movie_id IS NOT NULL)
        )
    );

INSERT INTO
    pending_new (id, user_id, timestamp, show_id, episode_id, movie_id)
SELECT
    id,
    (SELECT id FROM migration_owner),
    timestamp,
    show_id,
    episode_id,
    movie_id
FROM
    pending;

DROP TABLE pending;

ALTER TABLE pending_new
RENAME TO pending;

CREATE INDEX idx_pending_timestamp ON pending (user_id, timestamp);

CREATE UNIQUE INDEX idx_pending_show ON pending (user_id, show_id)
WHERE
    show_id IS NOT NULL;

CREATE UNIQUE INDEX idx_pending_movie ON pending (user_id, movie_id)
WHERE
    movie_id IS NOT NULL;

DROP TABLE migration_owner;
