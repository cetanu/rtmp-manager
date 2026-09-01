CREATE TABLE admin_audit_log (
    id TEXT PRIMARY KEY,
    actor_user_id TEXT NOT NULL,
    action TEXT NOT NULL,
    tenant_id TEXT,
    created_at BIGINT NOT NULL,
    FOREIGN KEY (actor_user_id) REFERENCES users(id)
);

CREATE INDEX admin_audit_log_created_at ON admin_audit_log (created_at);
