CREATE TABLE series (
    id        INTEGER PRIMARY KEY,
    title     TEXT NOT NULL,
    first_air INTEGER,
    overview  TEXT NOT NULL DEFAULT '',
    poster    TEXT,
    banner    TEXT,
    fanart    TEXT,
    tracked   INTEGER NOT NULL DEFAULT 1,
    remote_id TEXT
);

CREATE TABLE seasons (
    id        INTEGER PRIMARY KEY,
    series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    number    INTEGER NOT NULL,
    air_date  INTEGER,
    name      TEXT,
    overview  TEXT NOT NULL DEFAULT '',
    poster    TEXT,
    UNIQUE(series_id, number)
);

CREATE TABLE episodes (
    id              INTEGER PRIMARY KEY,
    series_id       INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    season          INTEGER NOT NULL,
    number          INTEGER NOT NULL,
    absolute_number INTEGER,
    name            TEXT,
    overview        TEXT NOT NULL DEFAULT '',
    aired           INTEGER,
    filename        TEXT,
    remote_id       TEXT,
    UNIQUE(series_id, season, number)
);

CREATE INDEX idx_episodes_aired ON episodes (aired) WHERE aired IS NOT NULL;

CREATE TABLE movies (
    id           INTEGER PRIMARY KEY,
    title        TEXT NOT NULL,
    release_date INTEGER,
    overview     TEXT NOT NULL DEFAULT '',
    poster       TEXT,
    banner       TEXT,
    fanart       TEXT,
    remote_id    TEXT
);

CREATE INDEX idx_movies_release_date ON movies (release_date) WHERE release_date IS NOT NULL;

CREATE TABLE watched (
    id         INTEGER PRIMARY KEY,
    timestamp  INTEGER NOT NULL,
    kind       TEXT NOT NULL CHECK(kind IN ('episode','movie')),
    series_id  INTEGER REFERENCES series(id) ON DELETE CASCADE,
    episode_id INTEGER REFERENCES episodes(id) ON DELETE CASCADE,
    movie_id   INTEGER REFERENCES movies(id) ON DELETE CASCADE
);

CREATE TABLE config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
