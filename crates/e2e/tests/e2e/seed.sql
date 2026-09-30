-- A small library for tests that need media, applied after the server has
-- created the schema. Languages are packed locales (1701734144 is English),
-- string kinds are 1 (title) and 2 (overview), timestamps are milliseconds.

INSERT INTO shows (id, first_air, tracked, default_language, auto_sync)
VALUES (1001, 1700000000000, 1, 1701734144, 0);

INSERT INTO show_strings (show_id, language, kind, text)
VALUES
    (1001, 1701734144, 1, 'Seeded Show'),
    (1001, 1701734144, 2, 'A show made up for the browser tests.');

INSERT INTO seasons (id, show_id, season, air_date)
VALUES (2001, 1001, 1, 1700000000000);

INSERT INTO episodes (id, show_id, season, episode, aired)
VALUES
    (3001, 1001, 1, 1, 1700000000000),
    (3002, 1001, 1, 2, 1700604800000),
    (3003, 1001, 1, 3, 1701209600000);

INSERT INTO episode_strings (episode_id, language, kind, text)
VALUES
    (3001, 1701734144, 1, 'First Episode'),
    (3002, 1701734144, 1, 'Second Episode'),
    (3003, 1701734144, 1, 'Third Episode');

-- The first episode is up next on the dashboard.
INSERT INTO pending (timestamp, show_id, episode_id)
VALUES (1700000000000, 1001, 3001);

-- A person, named in English with a country as synced names are.
INSERT INTO people (id, department, default_language)
VALUES (5001, 'Acting', 6148257917992593152);

INSERT INTO person_strings (person_id, language, kind, text)
VALUES (5001, 6148257917992593152, 1, 'Ada Lovelace');

-- A person with no default language, named only in Swedish (1937204480).
INSERT INTO people (id, department, default_language)
VALUES (5002, 'Acting', 0);

INSERT INTO person_strings (person_id, language, kind, text)
VALUES (5002, 1937204480, 1, 'Greta Garbo');
