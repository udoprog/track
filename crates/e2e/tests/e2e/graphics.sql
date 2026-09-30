-- Two poster candidates for the seeded show (kind 1 poster, source 2 TMDB),
-- the second chosen by the user. The paths are never fetched.

INSERT INTO show_image_candidates (id, show_id, kind, source, path, width, height, rank, score)
VALUES
    (7001, 1001, 1, 2, '/first.jpg', 500, 750, 0, 5.2),
    (7002, 1001, 1, 2, '/second.jpg', 500, 750, 1, 4.9);

INSERT INTO show_images (show_id, kind, image_id, user_selected)
VALUES (1001, 1, 7002, 1);
