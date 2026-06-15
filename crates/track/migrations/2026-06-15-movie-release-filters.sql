-- Per-movie override of which release types/countries determine the release date.
-- NULL means use the global default from the config table.
ALTER TABLE movies ADD COLUMN release_filters TEXT;
