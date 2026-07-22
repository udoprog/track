-- Split the single shared `images` pool (nullable show_id/movie_id/episode_id/
-- season_id + CHECK) into one candidate table per owner. The paired `*_images`
-- selection tables are unchanged in shape but their `image_id` foreign key now
-- points at the owner's candidate table, so they are rebuilt too.
--
-- Migrations run with `PRAGMA foreign_keys = OFF` (it is enabled only after this
-- pass), so dropping `images` while the selection tables still reference it is
-- safe; each selection table is rebuilt to repoint its FK before FKs are enabled.
-- Forward-only, data-preserving: image ids are carried through unchanged, so the
-- existing selection rows keep matching their candidate.

CREATE TABLE
    show_image_candidates (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        source INTEGER NOT NULL,
        path TEXT NOT NULL,
        width INTEGER NOT NULL,
        height INTEGER NOT NULL,
        rank INTEGER NOT NULL DEFAULT 0,
        score REAL,
        UNIQUE (show_id, kind, path)
    );

CREATE INDEX idx_show_image_candidates_rank ON show_image_candidates (show_id, kind, rank);

CREATE TABLE
    movie_image_candidates (
        id INTEGER PRIMARY KEY,
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        source INTEGER NOT NULL,
        path TEXT NOT NULL,
        width INTEGER NOT NULL,
        height INTEGER NOT NULL,
        rank INTEGER NOT NULL DEFAULT 0,
        score REAL,
        UNIQUE (movie_id, kind, path)
    );

CREATE INDEX idx_movie_image_candidates_rank ON movie_image_candidates (movie_id, kind, rank);

CREATE TABLE
    episode_image_candidates (
        id INTEGER PRIMARY KEY,
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        source INTEGER NOT NULL,
        path TEXT NOT NULL,
        width INTEGER NOT NULL,
        height INTEGER NOT NULL,
        rank INTEGER NOT NULL DEFAULT 0,
        score REAL,
        UNIQUE (episode_id, kind, path)
    );

CREATE TABLE
    season_image_candidates (
        id INTEGER PRIMARY KEY,
        season_id INTEGER NOT NULL REFERENCES seasons (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        source INTEGER NOT NULL,
        path TEXT NOT NULL,
        width INTEGER NOT NULL,
        height INTEGER NOT NULL,
        rank INTEGER NOT NULL DEFAULT 0,
        score REAL,
        UNIQUE (season_id, kind, path)
    );

CREATE INDEX idx_season_image_candidates_rank ON season_image_candidates (season_id, kind, rank);

INSERT INTO
    show_image_candidates (id, show_id, kind, source, path, width, height, rank, score)
SELECT
    id, show_id, kind, source, path, width, height, rank, score
FROM
    images
WHERE
    show_id IS NOT NULL;

INSERT INTO
    movie_image_candidates (id, movie_id, kind, source, path, width, height, rank, score)
SELECT
    id, movie_id, kind, source, path, width, height, rank, score
FROM
    images
WHERE
    movie_id IS NOT NULL;

INSERT INTO
    episode_image_candidates (id, episode_id, kind, source, path, width, height, rank, score)
SELECT
    id, episode_id, kind, source, path, width, height, rank, score
FROM
    images
WHERE
    episode_id IS NOT NULL;

INSERT INTO
    season_image_candidates (id, season_id, kind, source, path, width, height, rank, score)
SELECT
    id, season_id, kind, source, path, width, height, rank, score
FROM
    images
WHERE
    season_id IS NOT NULL;

-- Rebuild the selection tables to repoint image_id at the candidate tables.
CREATE TABLE
    show_images_new (
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES show_image_candidates (id) ON DELETE CASCADE,
        user_selected INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (show_id, kind)
    );

INSERT INTO
    show_images_new (show_id, kind, image_id, user_selected)
SELECT
    show_id, kind, image_id, user_selected
FROM
    show_images;

DROP TABLE show_images;

ALTER TABLE show_images_new RENAME TO show_images;

CREATE TABLE
    movie_images_new (
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES movie_image_candidates (id) ON DELETE CASCADE,
        user_selected INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (movie_id, kind)
    );

INSERT INTO
    movie_images_new (movie_id, kind, image_id, user_selected)
SELECT
    movie_id, kind, image_id, user_selected
FROM
    movie_images;

DROP TABLE movie_images;

ALTER TABLE movie_images_new RENAME TO movie_images;

CREATE TABLE
    episode_images_new (
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES episode_image_candidates (id) ON DELETE CASCADE,
        PRIMARY KEY (episode_id, kind)
    );

INSERT INTO
    episode_images_new (episode_id, kind, image_id)
SELECT
    episode_id, kind, image_id
FROM
    episode_images;

DROP TABLE episode_images;

ALTER TABLE episode_images_new RENAME TO episode_images;

CREATE TABLE
    season_images_new (
        season_id INTEGER NOT NULL REFERENCES seasons (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES season_image_candidates (id) ON DELETE CASCADE,
        PRIMARY KEY (season_id, kind)
    );

INSERT INTO
    season_images_new (season_id, kind, image_id)
SELECT
    season_id, kind, image_id
FROM
    season_images;

DROP TABLE season_images;

ALTER TABLE season_images_new RENAME TO season_images;

DROP TABLE images;
