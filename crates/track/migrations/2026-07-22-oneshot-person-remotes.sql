-- Give people the same remotes model as shows/movies. The old `people` row carried
-- an inline (source, remote_id) identity and a separate single-ETag `person_cache`
-- table; both are replaced by a `person_remotes` table shaped exactly like
-- show_remotes/movie_remotes (priority, enable, per-remote sync-kind override, and
-- an api::RemoteCache JSON in `cache`), plus a primary `remote_id` pointer on people.
--
-- Migrations run with `PRAGMA foreign_keys = OFF` (enabled only after this pass), so
-- rebuilding `people` while show_credits/movie_credits still reference it is safe;
-- credit ids and person ids are carried through unchanged. Forward-only: the old
-- per-person ETag in `person_cache` is dropped (re-fetched on the next sync); the
-- new RemoteCache state starts empty.

-- Seed person_remotes from the old inline identity. The old `people.remote_id` was
-- the source-specific numeric id, which is the remote's `value`.
CREATE TABLE
    person_remotes (
        id INTEGER PRIMARY KEY,
        person_id INTEGER NOT NULL,
        source INTEGER NOT NULL,
        value ANY NOT NULL,
        slug TEXT,
        enabled INTEGER NOT NULL DEFAULT 1,
        priority INTEGER NOT NULL DEFAULT 0,
        sync_kinds INTEGER,
        cache TEXT,
        UNIQUE (person_id, source, value)
    );

INSERT INTO
    person_remotes (person_id, source, value, enabled, priority)
SELECT
    id, source, remote_id, 1, 0
FROM
    people;

-- Promote each person's imdb cross-reference to a proper IMDb remote, matching how
-- shows/movies carry IMDb (source 3 = api::RemoteSource::Imdb); it was previously a
-- scalar column not listed among the person's remotes.
INSERT INTO
    person_remotes (person_id, source, value, enabled, priority)
SELECT
    id, 3, imdb_id, 1, 1
FROM
    people
WHERE
    imdb_id IS NOT NULL AND imdb_id <> '';

-- Rebuild people without the inline identity or the imdb_id column, pointing
-- remote_id at the primary (source) person_remotes row.
CREATE TABLE
    people_new (
        id INTEGER PRIMARY KEY,
        department TEXT,
        default_language INTEGER NOT NULL DEFAULT 0,
        last_synced_at INTEGER,
        remote_id INTEGER REFERENCES person_remotes (id) ON DELETE SET NULL
    );

INSERT INTO
    people_new (id, department, last_synced_at, remote_id)
SELECT
    p.id,
    p.department,
    p.last_synced_at,
    (
        SELECT pr.id
        FROM person_remotes pr
        WHERE pr.person_id = p.id AND pr.source = p.source
        LIMIT 1
    )
FROM
    people p;

DROP TABLE people;

ALTER TABLE people_new RENAME TO people;

CREATE INDEX idx_people_sync ON people (last_synced_at);

DROP TABLE person_cache;
