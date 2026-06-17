-- Separate "automatic sync" from "tracked". `tracked` now only controls whether
-- an item shows a pending episode on the dashboard; `auto_sync` controls whether
-- the background loop refreshes it. Seed auto_sync from the existing tracked value
-- so current behavior is preserved for existing rows.
ALTER TABLE shows  ADD COLUMN auto_sync INTEGER NOT NULL DEFAULT 1;
ALTER TABLE movies ADD COLUMN auto_sync INTEGER NOT NULL DEFAULT 1;
UPDATE shows  SET auto_sync = tracked;
UPDATE movies SET auto_sync = tracked;
