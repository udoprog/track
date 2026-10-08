-- The seeded show on TMDB (source 2), with a key so its sync goes to the slow
-- stand-in in `tmdb.rs`.
INSERT INTO show_remotes (show_id, source, value) VALUES (1001, 2, 77);
INSERT OR REPLACE INTO config (key, value) VALUES ('tmdb_api_key', 'e2e');
