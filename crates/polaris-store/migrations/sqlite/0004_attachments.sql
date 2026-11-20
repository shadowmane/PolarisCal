-- Files attached to Instruments/Systems and their calibration records.
-- The files live under the shared attachments folder (app_settings 'attachments_root');
-- rows keep the path relative to that folder so the share can be moved, plus a
-- SHA-256 fingerprint so later changes to the file can be detected.

CREATE TABLE attachments (
    id             INTEGER PRIMARY KEY,
    asset_id       INTEGER NOT NULL REFERENCES assets (id),
    calibration_id INTEGER REFERENCES calibration_records (id),
    category       TEXT    NOT NULL CHECK (category IN ('certificate', 'document', 'photo', 'other')),
    file_name      TEXT    NOT NULL,
    relative_path  TEXT    NOT NULL,
    size_bytes     INTEGER NOT NULL CHECK (size_bytes >= 0),
    sha256         TEXT    NOT NULL CHECK (length(sha256) = 64),
    content_type   TEXT,
    description    TEXT,
    uploaded_by_id INTEGER REFERENCES users (id),
    uploaded_by    TEXT    NOT NULL,
    is_archived    INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    row_version    INTEGER NOT NULL DEFAULT 1,
    created_at     TEXT    NOT NULL,
    updated_at     TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_attachments_path ON attachments (relative_path COLLATE NOCASE);
CREATE INDEX ix_attachments_asset ON attachments (asset_id);
CREATE INDEX ix_attachments_calibration ON attachments (calibration_id);
