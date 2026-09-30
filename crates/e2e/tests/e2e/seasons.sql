-- More seasons for the show in seed.sql: specials, listed after the numbered
-- seasons, and a second season.

INSERT INTO seasons (id, show_id, season, air_date)
VALUES
    (2002, 1001, 0, 1690000000000),
    (2003, 1001, 2, 1720000000000);

INSERT INTO episodes (id, show_id, season, episode, aired)
VALUES
    (3101, 1001, 0, 1, 1690000000000),
    (3201, 1001, 2, 1, 1720000000000),
    (3202, 1001, 2, 2, 1720604800000);

INSERT INTO episode_strings (episode_id, language, kind, text)
VALUES
    (3101, 1701734144, 1, 'Behind the Scenes'),
    (3201, 1701734144, 1, 'Second Season Opener'),
    (3202, 1701734144, 1, 'Second Season Finale');
