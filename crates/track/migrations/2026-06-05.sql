-- The schema track stores *now*, not what it stored at the start: an empty
-- database is built by this alone and every dated migration is recorded without
-- being run. A schema change is therefore two edits — this file, and a dated
-- migration carrying the same change — which are never both applied to one
-- database. See `db::do_migrations`.

CREATE TABLE
    shows (
        id INTEGER PRIMARY KEY,
        first_air INTEGER,
        sync_source INTEGER,
        default_language INTEGER NOT NULL DEFAULT 0,
        last_synced_at INTEGER,
        auto_sync INTEGER NOT NULL DEFAULT 1,
        air_date_filters TEXT,
        remote_id INTEGER REFERENCES show_remotes (id) ON DELETE SET NULL
    );

CREATE TABLE
    seasons (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        season INTEGER NOT NULL,
        air_date INTEGER,
        UNIQUE (show_id, season)
    );

CREATE TABLE
    episodes (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        season INTEGER NOT NULL,
        episode INTEGER NOT NULL,
        absolute_number INTEGER,
        aired INTEGER,
        last_synced_at INTEGER,
        UNIQUE (show_id, season, episode)
    );

CREATE INDEX idx_episodes_aired ON episodes (aired)
WHERE
    aired IS NOT NULL;

-- Supports the air-window query that schedules hourly per-episode syncs.
CREATE INDEX idx_episodes_air_sync ON episodes (aired, last_synced_at)
WHERE
    aired IS NOT NULL;

-- Conditional-request state for a single episode, per source. Unlike
-- show_remotes/movie_remotes this is NOT a remote identifier: an episode is
-- addressed through its show's remote id plus (season, episode), so there is no
-- value/priority/enabled here. A row exists only for a source that actually hands
-- out a validator (TMDB ETag, TVDB lastUpdated), letting the next per-episode call
-- to that source be deduplicated. `cache` holds api::RemoteCache as JSON.
CREATE TABLE
    episode_cache (
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        source INTEGER NOT NULL,
        cache TEXT NOT NULL,
        PRIMARY KEY (episode_id, source)
    );

CREATE TABLE
    movies (
        id INTEGER PRIMARY KEY,
        release_date INTEGER,
        sync_source INTEGER,
        default_language INTEGER NOT NULL DEFAULT 0,
        last_synced_at INTEGER,
        release_filters TEXT,
        auto_sync INTEGER NOT NULL DEFAULT 1,
        remote_id INTEGER REFERENCES movie_remotes (id) ON DELETE SET NULL
    );

CREATE INDEX idx_movies_release_date ON movies (release_date)
WHERE
    release_date IS NOT NULL;

CREATE TABLE
    movie_releases (
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        source INTEGER NOT NULL DEFAULT 0,
        country INTEGER NOT NULL DEFAULT 0,
        release_type INTEGER NOT NULL,
        timestamp INTEGER NOT NULL,
        PRIMARY KEY (movie_id, source, country, release_type)
    );

CREATE INDEX idx_movie_releases_movie ON movie_releases (movie_id, release_type, timestamp);

-- Tracking, watch history and pending are per user; everything else is shared.
CREATE TABLE
    user_tracked_shows (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        PRIMARY KEY (user_id, show_id)
    );

CREATE INDEX idx_user_tracked_shows_show ON user_tracked_shows (show_id);

CREATE TABLE
    user_tracked_movies (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        PRIMARY KEY (user_id, movie_id)
    );

CREATE INDEX idx_user_tracked_movies_movie ON user_tracked_movies (movie_id);

CREATE TABLE
    watched_episodes (
        id INTEGER PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        timestamp INTEGER NOT NULL,
        show_id INTEGER,
        season INTEGER NOT NULL,
        episode INTEGER NOT NULL
    );

CREATE INDEX idx_watched_episodes_show ON watched_episodes (user_id, show_id, season, episode);

CREATE TABLE
    watched_movies (
        id INTEGER PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        timestamp INTEGER NOT NULL,
        movie_id INTEGER
    );

CREATE INDEX idx_watched_movies_movie ON watched_movies (user_id, movie_id);

CREATE TABLE
    pending (
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

CREATE INDEX idx_pending_timestamp ON pending (user_id, timestamp);

CREATE UNIQUE INDEX idx_pending_show ON pending (user_id, show_id)
WHERE
    show_id IS NOT NULL;

CREATE UNIQUE INDEX idx_pending_movie ON pending (user_id, movie_id)
WHERE
    movie_id IS NOT NULL;

CREATE TABLE
    config (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- Preferences, per user and per user and show/movie: one row per non-default
-- value, keyed by api::PreferenceKey and holding the value as JSON.
CREATE TABLE
    user_config (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        key TEXT NOT NULL,
        value TEXT NOT NULL CHECK (json_valid (value)),
        PRIMARY KEY (user_id, key)
    );

CREATE TABLE
    user_show_config (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        key TEXT NOT NULL,
        value TEXT NOT NULL CHECK (json_valid (value)),
        PRIMARY KEY (user_id, show_id, key)
    );

CREATE INDEX idx_user_show_config_show ON user_show_config (show_id);

CREATE TABLE
    user_movie_config (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        key TEXT NOT NULL,
        value TEXT NOT NULL CHECK (json_valid (value)),
        PRIMARY KEY (user_id, movie_id, key)
    );

CREATE INDEX idx_user_movie_config_movie ON user_movie_config (movie_id);

-- Candidate image pools, one per owner. Each holds every stored graphic for an
-- owner (ranked, optionally scored); the paired `*_images` selection table below
-- picks the active candidate per kind.
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

CREATE TABLE
    show_images (
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES show_image_candidates (id) ON DELETE CASCADE,
        user_selected INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (show_id, kind)
    );

CREATE TABLE
    movie_images (
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES movie_image_candidates (id) ON DELETE CASCADE,
        user_selected INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (movie_id, kind)
    );

CREATE TABLE
    episode_images (
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES episode_image_candidates (id) ON DELETE CASCADE,
        PRIMARY KEY (episode_id, kind)
    );

CREATE TABLE
    season_images (
        season_id INTEGER NOT NULL REFERENCES seasons (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        image_id INTEGER NOT NULL REFERENCES season_image_candidates (id) ON DELETE CASCADE,
        PRIMARY KEY (season_id, kind)
    );

-- Remotes are normalized per owner: a random id, a numeric source enum
-- (api::RemoteSource) and a dynamic (integer or text) value. The owning
-- show/movie/episode also points back at its selected remote via remote_id.
--
-- show_remotes/movie_remotes intentionally have NO foreign key on their owner:
-- remote identifiers must outlive deletion of the show/movie (so re-adding the
-- same id re-links them), so the owner column is a plain id. stays tied to its
-- episode and cascades.
CREATE TABLE
    show_remotes (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL,
        source INTEGER NOT NULL,
        value ANY NOT NULL,
        slug TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        priority INTEGER NOT NULL DEFAULT 0,
        sync_kinds INTEGER,
        cache TEXT,
        UNIQUE (show_id, source, value)
    );

CREATE TABLE
    movie_remotes (
        id INTEGER PRIMARY KEY,
        movie_id INTEGER NOT NULL,
        source INTEGER NOT NULL,
        value ANY NOT NULL,
        slug TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        priority INTEGER NOT NULL DEFAULT 0,
        sync_kinds INTEGER,
        cache TEXT,
        UNIQUE (movie_id, source, value)
    );

CREATE TABLE
    episode_releases (
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        source INTEGER NOT NULL,
        country INTEGER NOT NULL DEFAULT 0,
        network TEXT NOT NULL DEFAULT '',
        timestamp INTEGER NOT NULL,
        PRIMARY KEY (episode_id, source, country, network)
    );

CREATE TABLE
    state (
        id INTEGER PRIMARY KEY CHECK (id = 0),
        top_languages TEXT NOT NULL DEFAULT '[]'
    );

CREATE TABLE
    show_strings (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        language INTEGER NOT NULL,
        kind INTEGER NOT NULL,
        text TEXT NOT NULL,
        UNIQUE (show_id, language, kind)
    );

CREATE TABLE
    movie_strings (
        id INTEGER PRIMARY KEY,
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        language INTEGER NOT NULL,
        kind INTEGER NOT NULL,
        text TEXT NOT NULL,
        UNIQUE (movie_id, language, kind)
    );

CREATE TABLE
    episode_strings (
        id INTEGER PRIMARY KEY,
        episode_id INTEGER NOT NULL REFERENCES episodes (id) ON DELETE CASCADE,
        language INTEGER NOT NULL,
        kind INTEGER NOT NULL,
        text TEXT NOT NULL,
        UNIQUE (episode_id, language, kind)
    );

CREATE TABLE
    season_strings (
        id INTEGER PRIMARY KEY,
        season_id INTEGER NOT NULL REFERENCES seasons (id) ON DELETE CASCADE,
        language INTEGER NOT NULL,
        kind INTEGER NOT NULL,
        text TEXT NOT NULL,
        UNIQUE (season_id, language, kind)
    );

-- People are shared across shows and movies and synced independently (their own
-- `last_synced_at`). Identity is a random PersonId; the source-specific ids live
-- in `person_remotes`, mirroring show_remotes/movie_remotes, and the person points
-- back at its primary (selected) remote via remote_id. The localized name and
-- biography live in `person_strings`.
CREATE TABLE
    people (
        id INTEGER PRIMARY KEY,
        department TEXT,
        default_language INTEGER NOT NULL DEFAULT 0,
        last_synced_at INTEGER,
        remote_id INTEGER REFERENCES person_remotes (id) ON DELETE SET NULL
    );

CREATE INDEX idx_people_sync ON people (last_synced_at);

-- Per-person remotes, identical in shape to show_remotes/movie_remotes: the
-- source-specific id, priority ordering, enable/disable, a per-remote sync-kind
-- override, and the api::RemoteCache conditional-request state (JSON) in `cache`.
-- No foreign key on the owner, for the same reason as show_remotes (see above).
CREATE TABLE
    person_remotes (
        id INTEGER PRIMARY KEY,
        person_id INTEGER NOT NULL,
        source INTEGER NOT NULL,
        value ANY NOT NULL,
        slug TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        priority INTEGER NOT NULL DEFAULT 0,
        sync_kinds INTEGER,
        cache TEXT,
        UNIQUE (person_id, source, value)
    );

-- Per-language name (kind=Title) and biography (kind=Overview) for a person.
CREATE TABLE
    person_strings (
        id INTEGER PRIMARY KEY,
        person_id INTEGER NOT NULL REFERENCES people (id) ON DELETE CASCADE,
        language INTEGER NOT NULL,
        kind INTEGER NOT NULL,
        text TEXT NOT NULL,
        UNIQUE (person_id, language, kind)
    );

-- Profile photos reuse the per-owner candidate pattern (ImageKind::Profile),
-- ranked/scored like other artwork.
CREATE TABLE
    person_image_candidates (
        id INTEGER PRIMARY KEY,
        person_id INTEGER NOT NULL REFERENCES people (id) ON DELETE CASCADE,
        kind INTEGER NOT NULL,
        source INTEGER NOT NULL,
        path TEXT NOT NULL,
        width INTEGER NOT NULL,
        height INTEGER NOT NULL,
        rank INTEGER NOT NULL DEFAULT 0,
        score REAL,
        UNIQUE (person_id, kind, path)
    );

CREATE INDEX idx_person_image_candidates_rank ON person_image_candidates (person_id, kind, rank);

-- A credit links a person to a show/movie in one cast role or crew job. One row
-- per role/job; the character name is translated (see show_credit_strings).
CREATE TABLE
    show_credits (
        id INTEGER PRIMARY KEY,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        person_id INTEGER NOT NULL REFERENCES people (id) ON DELETE CASCADE,
        credit_type INTEGER NOT NULL,
        department TEXT,
        job TEXT,
        sort_order INTEGER,
        episode_count INTEGER
    );

CREATE INDEX idx_show_credits_show ON show_credits (show_id);

CREATE TABLE
    movie_credits (
        id INTEGER PRIMARY KEY,
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        person_id INTEGER NOT NULL REFERENCES people (id) ON DELETE CASCADE,
        credit_type INTEGER NOT NULL,
        department TEXT,
        job TEXT,
        sort_order INTEGER,
        episode_count INTEGER
    );

CREATE INDEX idx_movie_credits_movie ON movie_credits (movie_id);

CREATE TABLE
    show_credit_strings (
        id INTEGER PRIMARY KEY,
        credit_id INTEGER NOT NULL REFERENCES show_credits (id) ON DELETE CASCADE,
        language INTEGER NOT NULL,
        kind INTEGER NOT NULL,
        text TEXT NOT NULL,
        UNIQUE (credit_id, language, kind)
    );

CREATE TABLE
    movie_credit_strings (
        id INTEGER PRIMARY KEY,
        credit_id INTEGER NOT NULL REFERENCES movie_credits (id) ON DELETE CASCADE,
        language INTEGER NOT NULL,
        kind INTEGER NOT NULL,
        text TEXT NOT NULL,
        UNIQUE (credit_id, language, kind)
    );
-- Roles are 'admin' or 'regular'. A user without a password signs in through a
-- login link or Cloudflare Access.
CREATE TABLE
    users (
        id INTEGER PRIMARY KEY,
        login TEXT NOT NULL UNIQUE,
        email TEXT UNIQUE,
        role TEXT NOT NULL,
        password_hash TEXT,
        created_at INTEGER NOT NULL
    );

CREATE TABLE
    sessions (
        id TEXT PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL
    );

CREATE INDEX idx_sessions_user ON sessions (user_id);

CREATE TABLE
    login_tokens (
        id TEXT PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        expires_at INTEGER NOT NULL,
        used_at INTEGER
    );

CREATE INDEX idx_login_tokens_user ON login_tokens (user_id);

-- The administrator "root" with the password "root".
INSERT INTO
    users (login, role, password_hash, created_at)
VALUES
    (
        'root',
        'admin',
        '$2y$12$jxThPHsK8E/RT.IrJcuNfO6Deoc5a7DkfUZ0cLhM2TZsbPT0MvIwO',
        CAST(unixepoch ('subsec') * 1000 AS INTEGER)
    );
