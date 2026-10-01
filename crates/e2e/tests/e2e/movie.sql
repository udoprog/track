-- A movie on top of seed.sql, with Ada Lovelace in its cast.

INSERT INTO movies (id, release_date, default_language, auto_sync)
VALUES (4001, 1700000000000, 1701734144, 0);

INSERT INTO user_tracked_movies (user_id, movie_id)
VALUES ((SELECT id FROM users WHERE login = 'root'), 4001);

INSERT INTO movie_strings (movie_id, language, kind, text)
VALUES (4001, 1701734144, 1, 'Seeded Movie');

INSERT INTO movie_credits (movie_id, person_id, credit_type, department, sort_order)
VALUES (4001, 5001, 0, 'Acting', 0);

-- A digital release from TMDB (source 2, type 4), shown with its logo.
INSERT INTO movie_releases (movie_id, source, country, release_type, timestamp)
VALUES (4001, 2, 0, 4, 1700000000000);
