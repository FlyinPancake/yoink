CREATE TABLE auth_settings (
    singleton            INTEGER PRIMARY KEY CHECK (singleton = 1),
    admin_username       TEXT NOT NULL,
    password_hash        TEXT NOT NULL,
    must_change_password INTEGER NOT NULL DEFAULT 0,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    password_changed_at  TEXT
);

CREATE TABLE auth_sessions (
    id                 BLOB PRIMARY KEY UNIQUE,
    session_token_hash TEXT NOT NULL UNIQUE,
    created_at         TEXT NOT NULL,
    last_seen_at       TEXT NOT NULL,
    expires_at         TEXT NOT NULL
);

CREATE INDEX idx_auth_sessions_token_hash ON auth_sessions(session_token_hash);
CREATE INDEX idx_auth_sessions_expires_at ON auth_sessions(expires_at);
