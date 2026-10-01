-- Tracked shows on top of seed.sql for the next-episode filters: one watched
-- to its end, one with only a special left to watch, and one being rewatched
-- with its second episode next. The seeded show has a regular episode next and
-- no specials.

INSERT INTO shows (id, first_air, default_language, auto_sync)
VALUES
    (1003, 1700000000000, 1701734144, 0),
    (1004, 1700000000000, 1701734144, 0),
    (1005, 1700000000000, 1701734144, 0);

INSERT INTO user_tracked_shows (user_id, show_id)
VALUES
    ((SELECT id FROM users WHERE login = 'root'), 1003),
    ((SELECT id FROM users WHERE login = 'root'), 1004),
    ((SELECT id FROM users WHERE login = 'root'), 1005);

INSERT INTO show_strings (show_id, language, kind, text)
VALUES
    (1003, 1701734144, 1, 'Finished Show'),
    (1004, 1701734144, 1, 'Specials Show'),
    (1005, 1701734144, 1, 'Rewatch Show');

INSERT INTO episodes (id, show_id, season, episode, aired)
VALUES
    (3301, 1003, 1, 1, 1700000000000),
    (3401, 1004, 0, 1, 1700000000000),
    (3402, 1004, 1, 1, 1700000000000),
    (3501, 1005, 1, 1, 1700000000000),
    (3502, 1005, 1, 2, 1700000000000);

INSERT INTO watched_episodes (user_id, timestamp, show_id, season, episode)
VALUES
    ((SELECT id FROM users WHERE login = 'root'), 1700100000000, 1003, 1, 1),
    ((SELECT id FROM users WHERE login = 'root'), 1700100000000, 1004, 1, 1),
    ((SELECT id FROM users WHERE login = 'root'), 1700100000000, 1005, 1, 1),
    ((SELECT id FROM users WHERE login = 'root'), 1700200000000, 1005, 1, 2),
    ((SELECT id FROM users WHERE login = 'root'), 1700300000000, 1005, 1, 1);
