-- A chosen backdrop for the seeded show (kind 3 backdrop, source 2 TMDB). The
-- path is never fetched.

INSERT INTO show_image_candidates (id, show_id, kind, source, path, width, height, rank, score)
VALUES (7101, 1001, 3, 2, '/backdrop.jpg', 1280, 720, 0, 5.0);

INSERT INTO show_images (show_id, kind, image_id, user_selected)
VALUES (1001, 3, 7101, 1);
