CREATE TABLE users (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    email TEXT UNIQUE NOT NULL,
    password_hash TEXT,
    role TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    FOREIGN KEY (tenant_id) REFERENCES tenants(id)
);

ALTER TABLE tenants ADD COLUMN chat TEXT NOT NULL DEFAULT '{}';
ALTER TABLE tenants ADD COLUMN overlay TEXT NOT NULL DEFAULT '{}';
ALTER TABLE tenants ADD COLUMN overlay_key_digest TEXT;
CREATE UNIQUE INDEX tenants_overlay_key_digest ON tenants (overlay_key_digest);

DROP TABLE chat_messages;
DROP TABLE chat_seen;
DROP TABLE chat_state;

CREATE TABLE chat_messages (
    id BIGINT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source TEXT NOT NULL,
    external_id TEXT NOT NULL,
    author TEXT NOT NULL,
    text TEXT NOT NULL,
    avatar_url TEXT,
    sent_at TEXT,
    received_at_unix_ms BIGINT NOT NULL
);

CREATE INDEX chat_messages_tenant_id ON chat_messages (tenant_id, id);

CREATE TABLE chat_seen (
    id BIGINT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    source TEXT NOT NULL,
    external_id TEXT NOT NULL,
    UNIQUE (tenant_id, source, external_id)
);

CREATE INDEX chat_seen_tenant_id ON chat_seen (tenant_id, id);

CREATE TABLE chat_state (
    tenant_id TEXT PRIMARY KEY,
    dropped BIGINT NOT NULL
);

CREATE TABLE user_identities (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    subject TEXT NOT NULL,
    UNIQUE (provider, subject),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE TABLE user_sessions (
    token_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX user_sessions_user ON user_sessions (user_id);

CREATE TABLE oauth_attempts (
    state_digest TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    pkce_verifier TEXT NOT NULL,
    expires_at BIGINT NOT NULL
);
