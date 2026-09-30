-- A movie on top of seed.sql, with Ada Lovelace in its cast.

INSERT INTO movies (id, release_date, tracked, default_language, auto_sync)
VALUES (4001, 1700000000000, 1, 1701734144, 0);

INSERT INTO movie_strings (movie_id, language, kind, text)
VALUES (4001, 1701734144, 1, 'Seeded Movie');

INSERT INTO movie_credits (movie_id, person_id, credit_type, department, sort_order)
VALUES (4001, 5001, 0, 'Acting', 0);
