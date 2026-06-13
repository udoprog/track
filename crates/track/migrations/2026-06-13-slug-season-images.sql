ALTER TABLE show_remotes ADD COLUMN slug TEXT;
ALTER TABLE movie_remotes ADD COLUMN slug TEXT;

-- Add season_id to images and update the CHECK constraint.
-- SQLite cannot modify CHECK constraints via ALTER TABLE, so the table must be recreated.
CREATE TABLE images_new (
    id INTEGER PRIMARY KEY,
    kind INTEGER NOT NULL,
    source INTEGER NOT NULL,
    path TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    rank INTEGER NOT NULL DEFAULT 0,
    show_id INTEGER REFERENCES shows (id) ON DELETE CASCADE,
    movie_id INTEGER REFERENCES movies (id) ON DELETE CASCADE,
    episode_id INTEGER REFERENCES episodes (id) ON DELETE CASCADE,
    season_id INTEGER REFERENCES seasons (id) ON DELETE CASCADE,
    CHECK (
        (show_id IS NOT NULL)
        OR (movie_id IS NOT NULL)
        OR (episode_id IS NOT NULL)
        OR (season_id IS NOT NULL)
    )
);

INSERT INTO images_new (id, kind, source, path, width, height, rank, show_id, movie_id, episode_id)
    SELECT id, kind, source, path, width, height, rank, show_id, movie_id, episode_id FROM images;

DROP TABLE images;
ALTER TABLE images_new RENAME TO images;

CREATE UNIQUE INDEX idx_images_show ON images (show_id, kind, path) WHERE show_id IS NOT NULL;
CREATE UNIQUE INDEX idx_images_movie ON images (movie_id, kind, path) WHERE movie_id IS NOT NULL;
CREATE UNIQUE INDEX idx_images_episode ON images (episode_id, kind, path) WHERE episode_id IS NOT NULL;
CREATE UNIQUE INDEX idx_images_season ON images (season_id, kind, path) WHERE season_id IS NOT NULL;
CREATE INDEX idx_images_show_rank ON images (show_id, kind, rank) WHERE show_id IS NOT NULL;
CREATE INDEX idx_images_movie_rank ON images (movie_id, kind, rank) WHERE movie_id IS NOT NULL;
CREATE INDEX idx_images_season_rank ON images (season_id, kind, rank) WHERE season_id IS NOT NULL;

CREATE TABLE season_images (
    season_id INTEGER NOT NULL REFERENCES seasons (id) ON DELETE CASCADE,
    kind INTEGER NOT NULL,
    image_id INTEGER NOT NULL REFERENCES images (id) ON DELETE CASCADE,
    PRIMARY KEY (season_id, kind)
);
