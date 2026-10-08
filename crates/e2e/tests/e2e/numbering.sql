-- Two more episodes for the seeded show, which XEM numbers as TheTVDB S1 E1-3
-- and S2 E1-2, so its five-episode first season does not line up.

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
    (1001, 4, 'anidb', 0, 1, 5, 5);
