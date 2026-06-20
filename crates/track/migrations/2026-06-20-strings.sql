-- Per-language translated strings for each media entity. The language column
-- always stores a concrete language (never the Language::DEFAULT sentinel); kind
-- is an integer enum (api::StringKind). Each row cascades with its owner. The
-- UNIQUE constraint also serves as the per-owner lookup index.
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

-- The entity's own original/default language as discovered during sync (distinct
-- from the existing `language` preference column). 0 means "not yet known".
ALTER TABLE shows ADD COLUMN default_language INTEGER NOT NULL DEFAULT 0;

ALTER TABLE movies ADD COLUMN default_language INTEGER NOT NULL DEFAULT 0;
