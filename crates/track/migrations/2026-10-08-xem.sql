-- XEM's numbering map for a show: one row per system in each map/all entry.
-- `entry` is the entry's index in map/all, `system` is XEM's name verbatim
-- (tvdb, scene, anidb, ...), and `part` is 1 for a double episode's second
-- address (XEM's "tvdb_2"), otherwise 0.
CREATE TABLE
    xem_episodes (
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        entry INTEGER NOT NULL,
        system TEXT NOT NULL,
        part INTEGER NOT NULL,
        season INTEGER NOT NULL,
        episode INTEGER NOT NULL,
        absolute INTEGER,
        PRIMARY KEY (show_id, entry, system, part)
    );

CREATE INDEX idx_xem_episodes_lookup ON xem_episodes (show_id, system, season, episode);

-- XEM's alternative names for a show (season NULL) or one of its seasons, in
-- the numbering of the show's XEM origin. `language` is XEM's code (us, jp, ...).
CREATE TABLE
    xem_names (
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        season INTEGER,
        language TEXT,
        name TEXT NOT NULL,
        UNIQUE (show_id, season, language, name)
    );
