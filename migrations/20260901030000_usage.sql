CREATE TABLE tenant_usage (
    tenant_id TEXT NOT NULL,
    period_start BIGINT NOT NULL,
    plan TEXT NOT NULL DEFAULT 'free',
    stream_seconds BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (tenant_id, period_start)
);

CREATE TABLE tenant_active_streams (
    tenant_id TEXT NOT NULL,
    stream_id TEXT PRIMARY KEY,
    started_at BIGINT NOT NULL
);
