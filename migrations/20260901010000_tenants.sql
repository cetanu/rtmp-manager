CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    stream_key_digest TEXT UNIQUE,
    active BIGINT NOT NULL,
    max_concurrent_streams BIGINT NOT NULL,
    notifications TEXT NOT NULL
);

CREATE TABLE tenant_targets (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    position BIGINT NOT NULL,
    config TEXT NOT NULL,
    UNIQUE (tenant_id, position),
    FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX tenant_targets_tenant_position
    ON tenant_targets (tenant_id, position);
