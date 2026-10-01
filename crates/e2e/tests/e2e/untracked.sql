-- A show on top of seed.sql that root does not track.

INSERT INTO shows (id, first_air, default_language, auto_sync)
VALUES (1002, 1700000000000, 1701734144, 0);

INSERT INTO show_strings (show_id, language, kind, text)
VALUES (1002, 1701734144, 1, 'Untracked Show');
