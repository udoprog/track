-- Add `score` to `images` (raw per-remote graphic score, used only for sorting
-- within a single remote) and `user_selected` to the show/movie selection
-- tables (marks selections the user explicitly made so sync leaves them alone).
-- All three are nullable/defaulted additive columns, so a plain ADD COLUMN
-- suffices - no table rebuild. Existing scores are left NULL and self-heal on
-- the next sync.
ALTER TABLE images ADD COLUMN score REAL;

ALTER TABLE show_images ADD COLUMN user_selected INTEGER NOT NULL DEFAULT 0;

ALTER TABLE movie_images ADD COLUMN user_selected INTEGER NOT NULL DEFAULT 0;
