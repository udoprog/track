CREATE TABLE
    series (
        id INTEGER PRIMARY KEY,
        title TEXT,
        first_air INTEGER,
        overview TEXT,
        tracked INTEGER NOT NULL DEFAULT 1,
        sync_source INTEGER,
        language TEXT,
        last_synced_at INTEGER
    );

CREATE TABLE
    seasons (
        id INTEGER PRIMARY KEY,
        series_id INTEGER NOT NULL REFERENCES series (id) ON DELETE CASCADE,
        number INTEGER NOT NULL,
        air_date INTEGER,
        name TEXT,
        overview TEXT,
        UNIQUE (series_id, number)
    );

CREATE TABLE
    episodes (
        id INTEGER PRIMARY KEY,
        series_id INTEGER NOT NULL REFERENCES series (id) ON DELETE CASCADE,
        season INTEGER NOT NULL,
        number INTEGER NOT NULL,
        absolute_number INTEGER,
        name TEXT,
        overview TEXT,
        aired INTEGER,
        remote_id TEXT,
        UNIQUE (series_id, season, number)
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
        series_id INTEGER,
        season INTEGER NOT NULL,
        episode INTEGER NOT NULL
    );

CREATE INDEX idx_watched_episodes_series ON watched_episodes (series_id, season, episode);

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
        series_id INTEGER REFERENCES series (id) ON DELETE CASCADE,
        episode_id INTEGER REFERENCES episodes (id) ON DELETE CASCADE,
        movie_id INTEGER REFERENCES movies (id) ON DELETE CASCADE,
        CHECK (
            (
                series_id IS NOT NULL
                AND episode_id IS NOT NULL
            )
            OR (movie_id IS NOT NULL)
        )
    );

CREATE INDEX idx_pending_timestamp ON pending (timestamp);

CREATE UNIQUE INDEX idx_pending_series ON pending (series_id)
WHERE
    series_id IS NOT NULL;

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
        series_id INTEGER REFERENCES series (id) ON DELETE CASCADE,
        movie_id INTEGER REFERENCES movies (id) ON DELETE CASCADE,
        episode_id INTEGER REFERENCES episodes (id) ON DELETE CASCADE,
        CHECK (
            (series_id IS NOT NULL)
            OR (movie_id IS NOT NULL)
            OR (episode_id IS NOT NULL)
        )
    );

CREATE UNIQUE INDEX idx_images_series ON images (series_id, kind, path)
WHERE
    series_id IS NOT NULL;

CREATE UNIQUE INDEX idx_images_movie ON images (movie_id, kind, path)
WHERE
    movie_id IS NOT NULL;

CREATE UNIQUE INDEX idx_images_episode ON images (episode_id, kind, path)
WHERE
    episode_id IS NOT NULL;

CREATE TABLE
    series_images (
        series_id INTEGER NOT NULL REFERENCES series (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES images (id) ON DELETE CASCADE,
        PRIMARY KEY (series_id, kind)
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
        series_id INTEGER,
        movie_id INTEGER,
        CHECK (
            (series_id IS NOT NULL)
            OR (movie_id IS NOT NULL)
        )
    );

CREATE UNIQUE INDEX idx_remotes_series ON remotes (series_id, remote_id)
WHERE
    series_id IS NOT NULL;

CREATE UNIQUE INDEX idx_remotes_movie ON remotes (movie_id, remote_id)
WHERE
    movie_id IS NOT NULL;