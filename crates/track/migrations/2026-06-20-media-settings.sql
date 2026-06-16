-- Per-media settings move off dedicated columns into a single JSON blob per
-- owner (api::ShowSettings / api::MovieSettings), so adding a setting no longer
-- needs a migration. No data backfill here: reads fall back to the legacy
-- columns (shows.language/include_specials/air_date_filters,
-- movies.language/release_filters) until the first settings write creates a row.
CREATE TABLE
    show_settings (
        show_id INTEGER PRIMARY KEY REFERENCES shows (id) ON DELETE CASCADE,
        data TEXT NOT NULL
    );

CREATE TABLE
    movie_settings (
        movie_id INTEGER PRIMARY KEY REFERENCES movies (id) ON DELETE CASCADE,
        data TEXT NOT NULL
    );
