-- Add cast & crew credits plus independently-synced people. All-new tables
-- (people + per-source cache + per-language name/biography + profile images, and
-- per-owner credits + character strings), so a plain create suffices - no rebuild.
-- On a fresh database the evolved baseline already has these, so this oneshot is
-- recorded as applied without executing.

CREATE TABLE
    people (
        id INTEGER PRIMARY KEY,
        source INTEGER NOT NULL,
        remote_id INTEGER NOT NULL,
        imdb_id TEXT,
        department TEXT,
        last_synced_at INTEGER,
        UNIQUE (source, remote_id)
    );

CREATE INDEX idx_people_sync ON people (last_synced_at);

CREATE TABLE
    person_cache (
        person_id INTEGER NOT NULL REFERENCES people (id) ON DELETE CASCADE,
        source INTEGER NOT NULL,
        cache TEXT NOT NULL,
        PRIMARY KEY (person_id, source)
    );

CREATE TABLE
    person_strings (
        id INTEGER PRIMARY KEY,
        person_id INTEGER NOT NULL REFERENCES people (id) ON DELETE CASCADE,
        language INTEGER NOT NULL,
        kind INTEGER NOT NULL,
        text TEXT NOT NULL,
        UNIQUE (person_id, language, kind)
    );

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
