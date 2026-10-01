-- Preferences become per user: one row per non-default value, the value as
-- JSON. What was set so far belongs to root (the first administrator if root
-- was renamed). Old values that do not parse are dropped, leaving the default.
CREATE TEMP TABLE migration_owner AS
SELECT
    id
FROM
    users
WHERE
    role = 'admin'
ORDER BY
    login <> 'root',
    id
LIMIT
    1;

CREATE TABLE
    user_config (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        key TEXT NOT NULL,
        value TEXT NOT NULL CHECK (json_valid (value)),
        PRIMARY KEY (user_id, key)
    );

CREATE TABLE
    user_show_config (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        show_id INTEGER NOT NULL REFERENCES shows (id) ON DELETE CASCADE,
        key TEXT NOT NULL,
        value TEXT NOT NULL CHECK (json_valid (value)),
        PRIMARY KEY (user_id, show_id, key)
    );

CREATE INDEX idx_user_show_config_show ON user_show_config (show_id);

CREATE TABLE
    user_movie_config (
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        movie_id INTEGER NOT NULL REFERENCES movies (id) ON DELETE CASCADE,
        key TEXT NOT NULL,
        value TEXT NOT NULL CHECK (json_valid (value)),
        PRIMARY KEY (user_id, movie_id, key)
    );

CREATE INDEX idx_user_movie_config_movie ON user_movie_config (movie_id);

-- Numbers were stored as decimal text, the rest as plain text.
CREATE TEMP TABLE migration_numbers AS
SELECT
    key,
    CAST(value AS INTEGER) AS value
FROM
    config
WHERE
    key IN (
        'dashboard_page',
        'dashboard_lookahead',
        'schedule_weeks',
        'schedule_range_days'
    )
    AND value <> ''
    AND value NOT GLOB '*[^0-9]*';

INSERT INTO
    user_config (user_id, key, value)
SELECT
    (SELECT id FROM migration_owner),
    key,
    value
FROM
    (
        SELECT
            'theme' AS key,
            json_quote (value) AS value
        FROM
            config
        WHERE
            key = 'theme'
            AND value IN ('light', 'system')
        UNION ALL
        SELECT
            'dashboard-page',
            CAST(value AS TEXT)
        FROM
            migration_numbers
        WHERE
            key = 'dashboard_page'
            AND value <> 5
        UNION ALL
        SELECT
            'dashboard-lookahead',
            CAST(value AS TEXT)
        FROM
            migration_numbers
        WHERE
            key = 'dashboard_lookahead'
            AND value <> 86400000
        UNION ALL
        SELECT
            'schedule-weeks',
            CAST(value AS TEXT)
        FROM
            migration_numbers
        WHERE
            key = 'schedule_weeks'
            AND value <> 4
        UNION ALL
        SELECT
            'schedule-range-days',
            CAST(value AS TEXT)
        FROM
            migration_numbers
        WHERE
            key = 'schedule_range_days'
            AND value <> 3
        UNION ALL
        SELECT
            'timezone',
            json_quote (value)
        FROM
            config
        WHERE
            key = 'timezone'
            AND value <> ''
        UNION ALL
        SELECT
            'language',
            json_quote (value)
        FROM
            config
        WHERE
            key = 'language'
            AND value NOT IN ('', 'default')
        UNION ALL
        SELECT
            'include-specials',
            'true'
        FROM
            config
        WHERE
            key = 'include_specials'
            AND value = 'true'
    );

DELETE FROM config
WHERE
    key IN (
        'theme',
        'dashboard_page',
        'dashboard_lookahead',
        'schedule_weeks',
        'schedule_range_days',
        'timezone',
        'language',
        'include_specials'
    );

DROP TABLE migration_numbers;

-- A packed locale (api::Locale::to_u64) holds the language's ASCII code in its
-- low four bytes and the country's in the high four, zero-padded; written out
-- as `language[-country]`, its text form.
CREATE TEMP TABLE migration_locales AS
WITH
    packed AS (
        SELECT
            'show' AS kind,
            id,
            language & 4294967295 AS l,
            (language >> 32) & 4294967295 AS c
        FROM
            shows
        WHERE
            language <> 0
        UNION ALL
        SELECT
            'movie',
            id,
            language & 4294967295,
            (language >> 32) & 4294967295
        FROM
            movies
        WHERE
            language <> 0
    ),
    codes AS (
        SELECT
            kind,
            id,
            iif (l >> 24 & 255, char(l >> 24 & 255), '') || iif (l >> 16 & 255, char(l >> 16 & 255), '') || iif (l >> 8 & 255, char(l >> 8 & 255), '') || iif (l & 255, char(l & 255), '') AS language,
            iif (c >> 24 & 255, char(c >> 24 & 255), '') || iif (c >> 16 & 255, char(c >> 16 & 255), '') || iif (c >> 8 & 255, char(c >> 8 & 255), '') || iif (c & 255, char(c & 255), '') AS country
        FROM
            packed
    )
SELECT
    kind,
    id,
    json_quote (language || iif (country = '', '', '-' || country)) AS value
FROM
    codes;

INSERT INTO
    user_show_config (user_id, show_id, key, value)
SELECT
    (SELECT id FROM migration_owner),
    id,
    'language',
    value
FROM
    migration_locales
WHERE
    kind = 'show';

INSERT INTO
    user_movie_config (user_id, movie_id, key, value)
SELECT
    (SELECT id FROM migration_owner),
    id,
    'language',
    value
FROM
    migration_locales
WHERE
    kind = 'movie';

INSERT INTO
    user_show_config (user_id, show_id, key, value)
SELECT
    (SELECT id FROM migration_owner),
    id,
    'include-specials',
    iif (include_specials, 'true', 'false')
FROM
    shows
WHERE
    include_specials IS NOT NULL;

DROP TABLE migration_locales;

ALTER TABLE shows
DROP COLUMN language;

ALTER TABLE shows
DROP COLUMN include_specials;

ALTER TABLE movies
DROP COLUMN language;

DROP TABLE migration_owner;
