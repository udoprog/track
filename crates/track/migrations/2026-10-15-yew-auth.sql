-- Moves the auth tables onto the schema of watchword's SqllStore (watchword was
-- then named yew-auth): its docs/sqll/track.sql as of e08325a.
--
-- `users.login` becomes optional, which SQLite can only do by rebuilding the
-- table. Dropping the old table with foreign keys on would cascade into every
-- table that references users, so the first statements abort the migration
-- (by a CHECK failure) unless foreign keys are off on this connection.
CREATE TEMP TABLE yew_auth_foreign_keys_off (foreign_keys INTEGER CHECK (foreign_keys = 0));

INSERT INTO
    yew_auth_foreign_keys_off
SELECT
    foreign_keys
FROM
    pragma_foreign_keys;

DROP TABLE yew_auth_foreign_keys_off;

-- Every existing user becomes active with a password method, so users without
-- a password keep signing in through Cloudflare Access or a login link.
CREATE TABLE
    users_new (
        id INTEGER PRIMARY KEY,
        login TEXT UNIQUE,
        email TEXT UNIQUE,
        role TEXT NOT NULL,
        access TEXT NOT NULL DEFAULT 'pending',
        method TEXT NOT NULL DEFAULT 'password',
        password_hash TEXT,
        google_sub TEXT UNIQUE,
        created_at INTEGER NOT NULL
    );

INSERT INTO
    users_new (
        id,
        login,
        email,
        role,
        access,
        method,
        password_hash,
        created_at
    )
SELECT
    id,
    login,
    email,
    role,
    'active',
    'password',
    password_hash,
    created_at
FROM
    users;

DROP TABLE users;

-- The tables referencing users (id) refer to the new table once it is renamed.
ALTER TABLE users_new
RENAME TO users;

-- Login links become reset links: their users are active now, and only reset
-- links can be used by an active user. Used links keep their used_at.
DROP INDEX idx_login_tokens_user;

ALTER TABLE login_tokens
RENAME TO auth_tokens;

ALTER TABLE auth_tokens
ADD COLUMN kind TEXT NOT NULL DEFAULT 'reset';

CREATE INDEX idx_auth_tokens_user ON auth_tokens (user_id);

-- sessions (INTEGER ms) and config (key, value) already match.
