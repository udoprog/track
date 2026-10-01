-- 25 more shows on top of seed.sql, so syncing them all fills more than one
-- page of the queue.

WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 25)
INSERT INTO shows (id, first_air, default_language, auto_sync)
SELECT 1100 + i, 1700000000000, 1701734144, 0 FROM n;

INSERT INTO user_tracked_shows (user_id, show_id)
SELECT (SELECT id FROM users WHERE login = 'root'), id FROM shows WHERE id > 1100;

WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 25)
INSERT INTO show_strings (show_id, language, kind, text)
SELECT 1100 + i, 1701734144, 1, 'Crowded Show ' || i FROM n;
