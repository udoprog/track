-- Roles are 'admin' or 'regular'. A user without a password signs in through a
-- login link or Cloudflare Access.
CREATE TABLE
    users (
        id INTEGER PRIMARY KEY,
        login TEXT NOT NULL UNIQUE,
        email TEXT UNIQUE,
        role TEXT NOT NULL,
        password_hash TEXT,
        created_at INTEGER NOT NULL
    );

CREATE TABLE
    sessions (
        id TEXT PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL,
        expires_at INTEGER NOT NULL
    );

CREATE INDEX idx_sessions_user ON sessions (user_id);

CREATE TABLE
    login_tokens (
        id TEXT PRIMARY KEY,
        user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
        expires_at INTEGER NOT NULL,
        used_at INTEGER
    );

CREATE INDEX idx_login_tokens_user ON login_tokens (user_id);

-- The administrator "root" with the password "root".
INSERT INTO
    users (login, role, password_hash, created_at)
VALUES
    (
        'root',
        'admin',
        '$2y$12$jxThPHsK8E/RT.IrJcuNfO6Deoc5a7DkfUZ0cLhM2TZsbPT0MvIwO',
        CAST(unixepoch ('subsec') * 1000 AS INTEGER)
    );
