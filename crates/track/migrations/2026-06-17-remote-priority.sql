-- Remotes gain an `enabled` flag and a `priority` (lower = higher priority). The
-- per-media set of enabled remotes ordered by priority replaces the single
-- `shows.sync_source`/`movies.sync_source` column (which is left dormant).
--
-- Priority ranking by source int (Tvdb=1, Tmdb=2, Imdb=3, Tvmaze=4): TVmaze ranks
-- highest so its air dates win by default; the remote matching the old sync_source
-- is preserved as the primary (just below TVmaze) so full-metadata sync keeps using it.
ALTER TABLE show_remotes ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE show_remotes ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE movie_remotes ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1;
ALTER TABLE movie_remotes ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;

UPDATE show_remotes SET priority = CASE
    WHEN source = (SELECT sync_source FROM shows WHERE shows.id = show_remotes.show_id) THEN 1
    WHEN source = 4 THEN 0   -- tvmaze
    WHEN source = 2 THEN 2   -- tmdb
    WHEN source = 1 THEN 3   -- tvdb
    WHEN source = 3 THEN 4   -- imdb
    ELSE 9 END;

UPDATE movie_remotes SET priority = CASE
    WHEN source = (SELECT sync_source FROM movies WHERE movies.id = movie_remotes.movie_id) THEN 1
    WHEN source = 4 THEN 0
    WHEN source = 2 THEN 2
    WHEN source = 1 THEN 3
    WHEN source = 3 THEN 4
    ELSE 9 END;
