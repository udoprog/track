CREATE TABLE
    series (
        id INTEGER PRIMARY KEY,
        title TEXT,
        first_air INTEGER,
        overview TEXT,
        tracked INTEGER NOT NULL DEFAULT 1,
        sync_source TEXT,
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
        poster TEXT,
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
        aired_date INTEGER,
        aired_timestamp INTEGER,
        filename TEXT,
        remote_id TEXT,
        UNIQUE (series_id, season, number)
    );

CREATE INDEX idx_episodes_aired_date ON episodes (aired_date)
WHERE
    aired_date IS NOT NULL;

CREATE INDEX idx_episodes_aired_timestamp ON episodes (aired_timestamp)
WHERE
    aired_timestamp IS NOT NULL;

CREATE TABLE
    movies (
        id INTEGER PRIMARY KEY,
        title TEXT,
        release_date INTEGER,
        overview TEXT,
        tracked INTEGER NOT NULL DEFAULT 1,
        sync_source TEXT,
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
        release_type TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        UNIQUE (movie_id, country, release_type)
    );

CREATE INDEX idx_movie_releases_movie ON movie_releases (movie_id);

CREATE INDEX idx_movie_releases_digital ON movie_releases (timestamp)
WHERE
    release_type = 'digital';

CREATE TABLE
    watched (
        id INTEGER PRIMARY KEY,
        timestamp INTEGER NOT NULL,
        episode_id INTEGER REFERENCES episodes (id) ON DELETE CASCADE,
        movie_id INTEGER REFERENCES movies (id) ON DELETE CASCADE,
        CHECK (
            (episode_id IS NOT NULL)
            OR (movie_id IS NOT NULL)
        )
    );

CREATE TABLE
    pending (
        id INTEGER PRIMARY KEY,
        timestamp INTEGER NOT NULL,
        series_id INTEGER REFERENCES series (id) ON DELETE CASCADE,
        episode_id INTEGER REFERENCES episodes (id) ON DELETE CASCADE,
        movie_id INTEGER REFERENCES movies (id) ON DELETE CASCADE,
        CHECK (
            (
                episode_id IS NULL
                AND series_id IS NOT NULL
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
        kind TEXT NOT NULL,
        source TEXT NOT NULL,
        path TEXT NOT NULL,
        selected INTEGER NOT NULL DEFAULT 0,
        series_id INTEGER REFERENCES series (id) ON DELETE CASCADE,
        movie_id INTEGER REFERENCES movies (id) ON DELETE CASCADE,
        CHECK (
            (series_id IS NOT NULL)
            OR (movie_id IS NOT NULL)
        )
    );

CREATE UNIQUE INDEX idx_images_series ON images (series_id, kind, path)
WHERE
    series_id IS NOT NULL;

CREATE UNIQUE INDEX idx_images_movie ON images (movie_id, kind, path)
WHERE
    movie_id IS NOT NULL;

CREATE TABLE
    remotes (
        id INTEGER PRIMARY KEY,
        remote_id TEXT NOT NULL,
        series_id INTEGER REFERENCES series (id) ON DELETE CASCADE,
        movie_id INTEGER REFERENCES movies (id) ON DELETE CASCADE,
        CHECK (
            (series_id IS NOT NULL)
            OR (movie_id IS NOT NULL)
        ),
        UNIQUE (remote_id)
    );

CREATE INDEX idx_remotes_series ON remotes (series_id)
WHERE
    series_id IS NOT NULL;

CREATE INDEX idx_remotes_movie ON remotes (movie_id)
WHERE
    movie_id IS NOT NULL;