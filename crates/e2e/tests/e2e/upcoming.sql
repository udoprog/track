-- A show with episodes airing tomorrow, for Upcoming and the Schedule. The
-- times are computed from the current time, so they are always ahead.

INSERT INTO shows (id, first_air, default_language, auto_sync)
VALUES (1002, 1700000000000, 1701734144, 0);

INSERT INTO user_tracked_shows (user_id, show_id)
VALUES ((SELECT id FROM users WHERE login = 'root'), 1002);

INSERT INTO show_strings (show_id, language, kind, text)
VALUES (1002, 1701734144, 1, 'Future Show');

INSERT INTO seasons (id, show_id, season, air_date)
VALUES (2101, 1002, 1, 1700000000000);

-- Tomorrow at 09:00 and 10:00 local time, so both fall on the same day.
INSERT INTO episodes (id, show_id, season, episode, aired)
VALUES
    (3401, 1002, 1, 1, CAST(strftime('%s', 'now', 'localtime', 'start of day', '+1 day', '+9 hours', 'utc') AS INTEGER) * 1000),
    (3402, 1002, 1, 2, CAST(strftime('%s', 'now', 'localtime', 'start of day', '+1 day', '+10 hours', 'utc') AS INTEGER) * 1000);

INSERT INTO episode_strings (episode_id, language, kind, text)
VALUES
    (3401, 1701734144, 1, 'Tomorrow Morning'),
    (3402, 1701734144, 1, 'An Hour Later');
