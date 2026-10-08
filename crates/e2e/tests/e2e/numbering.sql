-- Two more episodes for the seeded show, which XEM numbers as TheTVDB S1 E1-3
-- and S2 E1-2, so its five-episode first season does not line up. Scene
-- numbers like TheTVDB, AniDB numbers the same entries as S1 E1-5, Trakt
-- (hidden on episodes) has no first entry so its episode order disagrees,
-- TheTVDB's S2E2 is a double episode, and XEM knows other names for the
-- show and both of TheTVDB's seasons.

INSERT INTO episodes (id, show_id, season, episode, aired)
VALUES
    (3004, 1001, 1, 4, 1701814400000),
    (3005, 1001, 1, 5, 1702419200000);

INSERT INTO xem_episodes (show_id, entry, system, part, season, episode, absolute)
VALUES
    (1001, 0, 'tvdb', 0, 1, 1, 1),
    (1001, 1, 'tvdb', 0, 1, 2, 2),
    (1001, 2, 'tvdb', 0, 1, 3, 3),
    (1001, 3, 'tvdb', 0, 2, 1, 4),
    (1001, 4, 'tvdb', 0, 2, 2, 5),
    (1001, 0, 'anidb', 0, 1, 1, 1),
    (1001, 1, 'anidb', 0, 1, 2, 2),
    (1001, 2, 'anidb', 0, 1, 3, 3),
    (1001, 3, 'anidb', 0, 1, 4, 4),
    (1001, 4, 'anidb', 0, 1, 5, 5),
    (1001, 4, 'tvdb', 1, 2, 3, 6),
    (1001, 0, 'scene', 0, 1, 1, 1),
    (1001, 1, 'scene', 0, 1, 2, 2),
    (1001, 2, 'scene', 0, 1, 3, 3),
    (1001, 3, 'scene', 0, 2, 1, 4),
    (1001, 4, 'scene', 0, 2, 2, 5),
    (1001, 1, 'trakt', 0, 1, 1, 1),
    (1001, 2, 'trakt', 0, 1, 2, 2),
    (1001, 3, 'trakt', 0, 1, 3, 3),
    (1001, 4, 'trakt', 0, 1, 4, 4);

INSERT INTO xem_names (show_id, season, language, name)
VALUES
    (1001, NULL, 'us', 'seeded show'),
    (1001, NULL, 'jp', 'Shīdo Shō'),
    (1001, NULL, 'de', 'Die Testserie'),
    (1001, NULL, 'us', 'Seeded'),
    (1001, NULL, 'fr', 'La Série'),
    (1001, 1, 'jp', 'Seeded First'),
    (1001, 2, 'jp', 'Seeded Second');

-- The XEM remote sync found it through, with XEM's own show id as its slug.
INSERT INTO show_remotes (show_id, source, value, slug)
VALUES
    (1001, 5, 'tvdb/424536', '6743');
