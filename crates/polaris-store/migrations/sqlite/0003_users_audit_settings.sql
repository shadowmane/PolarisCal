-- Users, the audit trail, and organization settings.

CREATE TABLE users (
    id                   INTEGER PRIMARY KEY,
    username             TEXT    NOT NULL,
    display_name         TEXT    NOT NULL,
    email                TEXT,
    role                 TEXT    NOT NULL CHECK (role IN ('admin', 'technician', 'viewer')),
    password_hash        TEXT    NOT NULL,
    must_change_password INTEGER NOT NULL DEFAULT 0 CHECK (must_change_password IN (0, 1)),
    is_active            INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    last_sign_in_at      TEXT,
    row_version          INTEGER NOT NULL DEFAULT 1,
    created_at           TEXT    NOT NULL,
    updated_at           TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_users_username ON users (username COLLATE NOCASE);

-- Append-only: rows are inserted with the change they describe and never altered.
CREATE TABLE audit_log (
    id           INTEGER PRIMARY KEY,
    occurred_at  TEXT    NOT NULL,
    user_id      INTEGER REFERENCES users (id),
    username     TEXT    NOT NULL,
    action       TEXT    NOT NULL,
    entity       TEXT    NOT NULL,
    entity_id    INTEGER,
    asset_id     INTEGER,
    entity_label TEXT,
    reason       TEXT,
    changes      TEXT    NOT NULL DEFAULT '[]'   -- JSON array of {field, old, new}
);
CREATE INDEX ix_audit_log_asset ON audit_log (asset_id, id);
CREATE INDEX ix_audit_log_entity ON audit_log (entity, entity_id, id);
CREATE INDEX ix_audit_log_user ON audit_log (user_id, id);

CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'the audit log is append-only');
END;

CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'the audit log is append-only');
END;

CREATE TABLE app_settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO app_settings (key, value, updated_at)
VALUES ('idle_lock_minutes', '15', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
