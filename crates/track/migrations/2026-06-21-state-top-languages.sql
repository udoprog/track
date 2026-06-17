-- Singleton table for derived/global state that is recomputed periodically rather
-- than set by the user. `top_languages` is a JSON array of ISO-639-1 codes, the
-- most-used per-show/per-movie custom language overrides, ordered most-used first.
CREATE TABLE state (
    id INTEGER PRIMARY KEY CHECK (id = 0),
    top_languages TEXT NOT NULL DEFAULT '[]'
);

INSERT INTO state (id, top_languages) VALUES (0, '[]');
