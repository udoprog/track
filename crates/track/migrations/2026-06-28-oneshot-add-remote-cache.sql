-- Add a per-remote `cache` column holding source-specific conditional-request
-- state as JSON (TMDB ETag, TVDB lastUpdated). Nullable with no default, so a
-- plain ADD COLUMN suffices - no table rebuild. The value is transient: a sync
-- repopulates it, and an unparsable value is treated as "no cache".
ALTER TABLE show_remotes ADD COLUMN cache TEXT;

ALTER TABLE movie_remotes ADD COLUMN cache TEXT;
