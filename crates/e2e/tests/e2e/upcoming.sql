-- A show with episodes airing tomorrow, for Upcoming and the Schedule. The
-- times are computed from the current time, so they are always ahead.

INSERT INTO shows (id, first_air, tracked, default_language, auto_sync)
VALUES (1002, 1700000000000, 1, 1701734144, 0);

INSERT INTO show_strings (show_id, language, kind, text)
VALUES (1002, 1701734144, 1, 'Future Show');

INSERT INTO seasons (id, show_id, season, air_date)
VALUES (2101, 1002, 1, 1700000000000);

-- Tomorrow at this hour, and an hour later.
INSERT INTO episodes (id, show_id, season, episode, aired)
VALUES
    (3401, 1002, 1, 1, (CAST(strftime('%s', 'now') AS INTEGER) + 86400) * 1000),
    (3402, 1002, 1, 2, (CAST(strftime('%s', 'now') AS INTEGER) + 90000) * 1000);

INSERT INTO episode_strings (episode_id, language, kind, text)
VALUES
    (3401, 1701734144, 1, 'Tomorrow Morning'),
    (3402, 1701734144, 1, 'An Hour Later');
