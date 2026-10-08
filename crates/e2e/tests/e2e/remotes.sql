-- A TMDB remote for every show, so show pages offer Sync. Without a TMDB key
-- their syncs fail at once, without reaching the network.

INSERT INTO show_remotes (show_id, source, value)
SELECT id, 2, id FROM shows;
