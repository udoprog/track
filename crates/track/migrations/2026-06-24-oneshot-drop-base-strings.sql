-- Drop the redundant base `title`/`name`/`overview` columns from shows,
-- seasons, episodes and movies. All localized strings now live exclusively in
-- the `*_strings` tables, keyed by (entity_id, language, kind).

ALTER TABLE shows DROP COLUMN title;
ALTER TABLE shows DROP COLUMN overview;

ALTER TABLE seasons DROP COLUMN name;
ALTER TABLE seasons DROP COLUMN overview;

ALTER TABLE episodes DROP COLUMN name;
ALTER TABLE episodes DROP COLUMN overview;

ALTER TABLE movies DROP COLUMN title;
ALTER TABLE movies DROP COLUMN overview;
