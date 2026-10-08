-- A chosen backdrop for the movie in movie.sql (kind 3 backdrop, source 2
-- TMDB). The path is never fetched.

INSERT INTO movie_image_candidates (id, movie_id, kind, source, path, width, height, rank, score)
VALUES (7201, 4001, 3, 2, '/movie-backdrop.jpg', 1280, 720, 0, 5.0);

INSERT INTO movie_images (movie_id, kind, image_id, user_selected)
VALUES (4001, 3, 7201, 1);
