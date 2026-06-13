CREATE TABLE
    shows (
        id INTEGER PRIMARY KEY,
        title TEXT,
        first_air INTEGER,
        overview TEXT,
        tracked INTEGER NOT NULL DEFAULT 1,
        sync_source INTEGER,
        language TEXT,
        last_synced_at INTEGER,
        include_specials INTEGER
    );

CREATE TABLE
    seasons (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        season INTEGER NOT NULL,
        air_date INTEGER,
        name TEXT,
        overview TEXT,
        UNIQUE (show_id, season)
    );

CREATE TABLE
    episodes (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        season INTEGER NOT NULL,
        episode INTEGER NOT NULL,
        absolute_number INTEGER,
        name TEXT,
        overview TEXT,
        aired INTEGER,
        remote_id TEXT,
        UNIQUE (show_id, season, episode)
    );

CREATE INDEX idx_episodes_aired ON episodes (aired)
WHERE
    aired IS NOT NULL;

CREATE TABLE
    movies (
        id INTEGER PRIMARY KEY,
        title TEXT,
        release_date INTEGER,
        overview TEXT,
        tracked INTEGER NOT NULL DEFAULT 1,
        sync_source INTEGER,
        language TEXT,
        last_synced_at INTEGER
    );

CREATE INDEX idx_movies_release_date ON movies (release_date)
WHERE
    release_date IS NOT NULL;

CREATE TABLE
    movie_releases (
        id INTEGER PRIMARY KEY,
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        country TEXT NOT NULL,
        release_type INTEGER NOT NULL,
        timestamp INTEGER NOT NULL,
        UNIQUE (movie_id, country, release_type)
    );

CREATE INDEX idx_movie_releases_movie ON movie_releases (movie_id, release_type, timestamp);

CREATE TABLE
    watched_episodes (
        id INTEGER PRIMARY KEY,
        timestamp INTEGER NOT NULL,
        show_id INTEGER,
        season INTEGER NOT NULL,
        episode INTEGER NOT NULL
    );

CREATE INDEX idx_watched_episodes_show ON watched_episodes (show_id, season, episode);

CREATE TABLE
    watched_movies (
        id INTEGER PRIMARY KEY,
        timestamp INTEGER NOT NULL,
        movie_id INTEGER
    );

CREATE INDEX idx_watched_movies_movie ON watched_movies (movie_id);

CREATE TABLE
    pending (
        id INTEGER PRIMARY KEY,
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

CREATE INDEX idx_pending_timestamp ON pending (timestamp);

CREATE UNIQUE INDEX idx_pending_show ON pending (show_id)
WHERE
    show_id IS NOT NULL;

CREATE UNIQUE INDEX idx_pending_movie ON pending (movie_id)
WHERE
    movie_id IS NOT NULL;

CREATE TABLE
    config (key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE
    images (
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
        CHECK (
            (show_id IS NOT NULL)
            OR (movie_id IS NOT NULL)
            OR (episode_id IS NOT NULL)
        )
    );

CREATE UNIQUE INDEX idx_images_show ON images (show_id, kind, path)
WHERE
    show_id IS NOT NULL;

CREATE UNIQUE INDEX idx_images_movie ON images (movie_id, kind, path)
WHERE
    movie_id IS NOT NULL;

CREATE UNIQUE INDEX idx_images_episode ON images (episode_id, kind, path)
WHERE
    episode_id IS NOT NULL;

-- Ordering galleries best-first (lowest rank) within a kind.
CREATE INDEX idx_images_show_rank ON images (show_id, kind, rank)
WHERE
    show_id IS NOT NULL;

CREATE INDEX idx_images_movie_rank ON images (movie_id, kind, rank)
WHERE
    movie_id IS NOT NULL;

CREATE TABLE
    show_images (
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES images (id) ON DELETE CASCADE,
        PRIMARY KEY (show_id, kind)
    );

CREATE TABLE
    movie_images (
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES images (id) ON DELETE CASCADE,
        PRIMARY KEY (movie_id, kind)
    );

CREATE TABLE
    episode_images (
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES images (id) ON DELETE CASCADE,
        PRIMARY KEY (episode_id, kind)
    );

CREATE TABLE
    remotes (
        remote_id TEXT NOT NULL,
        show_id INTEGER,
        movie_id INTEGER,
        CHECK (
            (show_id IS NOT NULL)
            OR (movie_id IS NOT NULL)
        )
    );

CREATE UNIQUE INDEX idx_remotes_show ON remotes (show_id, remote_id)
WHERE
    show_id IS NOT NULL;

CREATE UNIQUE INDEX idx_remotes_movie ON remotes (movie_id, remote_id)
WHERE
    movie_id IS NOT NULL;