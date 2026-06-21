-- Convert the legacy `language` preference columns from TEXT to the integer enum
-- now expected by api::Language (its FromColumn reads an INTEGER). Existing text
-- codes are not preserved: the column is dropped and re-added as
-- INTEGER NOT NULL DEFAULT 0, where 0 is Language::DEFAULT ("use the media's own
-- original language"). The next sync repopulates default_language as needed.
--
-- Neither column is indexed or otherwise constrained, so DROP COLUMN is safe.
-- The re-added column lands at the end of the table; every query lists `language`
-- explicitly, so physical column order does not matter.
ALTER TABLE shows DROP COLUMN language;

ALTER TABLE shows ADD COLUMN language INTEGER NOT NULL DEFAULT 0;

ALTER TABLE movies DROP COLUMN language;

ALTER TABLE movies ADD COLUMN language INTEGER NOT NULL DEFAULT 0;
