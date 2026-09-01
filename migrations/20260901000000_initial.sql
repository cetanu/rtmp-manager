CREATE TABLE app_config (
    id BIGINT PRIMARY KEY,
    data TEXT NOT NULL
);

CREATE TABLE chat_messages (
    id BIGINT PRIMARY KEY,
    source TEXT NOT NULL,
    external_id TEXT NOT NULL,
    author TEXT NOT NULL,
    text TEXT NOT NULL,
    avatar_url TEXT,
    sent_at TEXT,
    received_at_unix_ms BIGINT NOT NULL
);

CREATE TABLE chat_seen (
    id BIGINT PRIMARY KEY,
    source TEXT NOT NULL,
    external_id TEXT NOT NULL,
    UNIQUE (source, external_id)
);

CREATE TABLE chat_state (
    id BIGINT PRIMARY KEY,
    dropped BIGINT NOT NULL
);

