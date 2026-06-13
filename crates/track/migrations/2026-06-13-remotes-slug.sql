ALTER TABLE shows
DROP COLUMN slug;

ALTER TABLE show_remotes
ADD COLUMN slug TEXT;

ALTER TABLE movie_remotes
ADD COLUMN slug TEXT;
