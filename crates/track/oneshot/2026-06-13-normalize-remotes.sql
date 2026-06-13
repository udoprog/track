-- Normalize the stringy `remotes.remote_id` / `episodes.remote_id` TEXT columns
-- into dedicated per-owner tables that store the source as a numeric enum and
-- the value as a dynamic (integer or text) column.
--
-- Migrations run before `PRAGMA foreign_keys = ON` (see do_migrations vs
-- ensure_mode), so foreign keys are NOT enforced here. This makes the episodes
-- table rebuild and the circular episodes <-> episode_remotes reference safe.
--
-- Source numeric encoding matches api::RemoteSource: unknown=0, tvdb=1, tmdb=2,
-- imdb=3. The `value` columns are declared without a type so they keep no
-- affinity and preserve the integer/text storage class written into them.

-- Databases predating this migration also predate the series -> shows rename
-- (the table, its `series_id` foreign keys and the `series_images` table were
-- renamed in the baseline without a dedicated migration). Bring that legacy
-- schema up to date first. Fresh databases already match the baseline and skip
-- this migration entirely (the baseline marks it applied), so these statements
-- only ever run against the old `series` schema.
ALTER TABLE series
RENAME TO shows;

ALTER TABLE series_images
RENAME TO show_images;

ALTER TABLE seasons
RENAME COLUMN series_id TO show_id;

ALTER TABLE episodes
RENAME COLUMN series_id TO show_id;

ALTER TABLE watched_episodes
RENAME COLUMN series_id TO show_id;

ALTER TABLE pending
RENAME COLUMN series_id TO show_id;

ALTER TABLE images
RENAME COLUMN series_id TO show_id;

ALTER TABLE show_images
RENAME COLUMN series_id TO show_id;

ALTER TABLE remotes
RENAME COLUMN series_id TO show_id;

-- Rename the series-scoped indexes to their show-scoped baseline names. (The
-- column references inside them were updated automatically by RENAME COLUMN;
-- only the index names need fixing. idx_remotes_series is dropped with the
-- remotes table below.)
DROP INDEX idx_watched_episodes_series;

DROP INDEX idx_pending_series;

DROP INDEX idx_images_series;

DROP INDEX idx_images_series_rank;

CREATE INDEX idx_watched_episodes_show ON watched_episodes (show_id, season, episode);

CREATE UNIQUE INDEX idx_pending_show ON pending (show_id)
WHERE
    show_id IS NOT NULL;

CREATE UNIQUE INDEX idx_images_show ON images (show_id, kind, path)
WHERE
    show_id IS NOT NULL;

CREATE INDEX idx_images_show_rank ON images (show_id, kind, rank)
WHERE
    show_id IS NOT NULL;

CREATE TABLE
    show_remotes (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL,
        source INTEGER NOT NULL,
        value,
        UNIQUE (show_id, source, value)
    );

CREATE TABLE
    movie_remotes (
        id INTEGER PRIMARY KEY,
        movie_id INTEGER NOT NULL,
        source INTEGER NOT NULL,
        value,
        UNIQUE (movie_id, source, value)
    );

CREATE TABLE
    episode_remotes (
        id INTEGER PRIMARY KEY,
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        source INTEGER NOT NULL,
        value,
        UNIQUE (episode_id, source, value)
    );

-- Migrate the shared `remotes` table into the show/movie tables, parsing the
-- "source:value" text and assigning each row a random identifier.
INSERT INTO
    show_remotes (id, show_id, source, value)
SELECT
    random(),
    show_id,
    CASE
        WHEN remote_id LIKE 'tvdb:%' THEN 1
        WHEN remote_id LIKE 'tmdb:%' THEN 2
        WHEN remote_id LIKE 'imdb:%' THEN 3
        ELSE 0
    END,
    CASE
        WHEN substr(remote_id, instr(remote_id, ':') + 1) GLOB '[0-9]*'
        AND substr(remote_id, instr(remote_id, ':') + 1) NOT GLOB '*[^0-9]*' THEN CAST(substr(remote_id, instr(remote_id, ':') + 1) AS INTEGER)
        ELSE substr(remote_id, instr(remote_id, ':') + 1)
    END
FROM
    remotes
WHERE
    show_id IS NOT NULL;

INSERT INTO
    movie_remotes (id, movie_id, source, value)
SELECT
    random(),
    movie_id,
    CASE
        WHEN remote_id LIKE 'tvdb:%' THEN 1
        WHEN remote_id LIKE 'tmdb:%' THEN 2
        WHEN remote_id LIKE 'imdb:%' THEN 3
        ELSE 0
    END,
    CASE
        WHEN substr(remote_id, instr(remote_id, ':') + 1) GLOB '[0-9]*'
        AND substr(remote_id, instr(remote_id, ':') + 1) NOT GLOB '*[^0-9]*' THEN CAST(substr(remote_id, instr(remote_id, ':') + 1) AS INTEGER)
        ELSE substr(remote_id, instr(remote_id, ':') + 1)
    END
FROM
    remotes
WHERE
    movie_id IS NOT NULL;

-- Migrate the inline episode remote.
INSERT INTO
    episode_remotes (id, episode_id, source, value)
SELECT
    random(),
    id,
    CASE
        WHEN remote_id LIKE 'tvdb:%' THEN 1
        WHEN remote_id LIKE 'tmdb:%' THEN 2
        WHEN remote_id LIKE 'imdb:%' THEN 3
        ELSE 0
    END,
    CASE
        WHEN substr(remote_id, instr(remote_id, ':') + 1) GLOB '[0-9]*'
        AND substr(remote_id, instr(remote_id, ':') + 1) NOT GLOB '*[^0-9]*' THEN CAST(substr(remote_id, instr(remote_id, ':') + 1) AS INTEGER)
        ELSE substr(remote_id, instr(remote_id, ':') + 1)
    END
FROM
    episodes
WHERE
    remote_id IS NOT NULL;

-- Selected-remote pointers on shows/movies. Prefer the remote whose source
-- matches the show/movie sync_source, otherwise the lowest id.
ALTER TABLE shows
ADD COLUMN remote_id INTEGER REFERENCES show_remotes (id) ON DELETE SET NULL;

UPDATE shows
SET
    remote_id = COALESCE(
        (
            SELECT
                sr.id
            FROM
                show_remotes sr
            WHERE
                sr.show_id = shows.id
                AND sr.source = shows.sync_source
            LIMIT
                1
        ),
        (
            SELECT
                sr.id
            FROM
                show_remotes sr
            WHERE
                sr.show_id = shows.id
            ORDER BY
                sr.id
            LIMIT
                1
        )
    );

ALTER TABLE movies
ADD COLUMN remote_id INTEGER REFERENCES movie_remotes (id) ON DELETE SET NULL;

UPDATE movies
SET
    remote_id = COALESCE(
        (
            SELECT
                mr.id
            FROM
                movie_remotes mr
            WHERE
                mr.movie_id = movies.id
                AND mr.source = movies.sync_source
            LIMIT
                1
        ),
        (
            SELECT
                mr.id
            FROM
                movie_remotes mr
            WHERE
                mr.movie_id = movies.id
            ORDER BY
                mr.id
            LIMIT
                1
        )
    );

-- Rebuild episodes so `remote_id` becomes an INTEGER foreign key into
-- episode_remotes (SQLite cannot retype a column in place).
CREATE TABLE
    episodes_new (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        season INTEGER NOT NULL,
        episode INTEGER NOT NULL,
        absolute_number INTEGER,
        name TEXT,
        overview TEXT,
        aired INTEGER,
        remote_id INTEGER REFERENCES episode_remotes (id) ON DELETE SET NULL,
        UNIQUE (show_id, season, episode)
    );

INSERT INTO
    episodes_new (id, show_id, season, episode, absolute_number, name, overview, aired, remote_id)
SELECT
    e.id,
    e.show_id,
    e.season,
    e.episode,
    e.absolute_number,
    e.name,
    e.overview,
    e.aired,
    (
        SELECT
            er.id
        FROM
            episode_remotes er
        WHERE
            er.episode_id = e.id
    )
FROM
    episodes e;

DROP TABLE episodes;

ALTER TABLE episodes_new
RENAME TO episodes;

CREATE INDEX idx_episodes_aired ON episodes (aired)
WHERE
    aired IS NOT NULL;

-- Drop the obsolete shared remotes table (its indexes go with it).
DROP TABLE remotes;
