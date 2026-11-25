-- no-transaction
-- Service manuals, and a folder per kind of file. Files can be kept in separate folders
-- (Settings > Attachments folders); each row records which one it was saved in. Files
-- attached until now are all in the main (certificates) folder. File paths only need to be
-- unique within their folder.
PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE attachments_new (
    id             INTEGER PRIMARY KEY,
    asset_id       INTEGER NOT NULL REFERENCES assets (id),
    calibration_id INTEGER REFERENCES calibration_records (id),
    category       TEXT    NOT NULL CHECK (category IN ('certificate', 'document', 'photo', 'service_manual', 'other')),
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
    updated_at     TEXT    NOT NULL,
    storage_folder TEXT    NOT NULL DEFAULT 'certificates' CHECK (storage_folder IN ('certificates', 'photos', 'service_manuals', 'documents'))
);
INSERT INTO attachments_new (id, asset_id, calibration_id, category, file_name, relative_path,
                             size_bytes, sha256, content_type, description, uploaded_by_id,
                             uploaded_by, is_archived, row_version, created_at, updated_at)
SELECT id, asset_id, calibration_id, category, file_name, relative_path, size_bytes, sha256,
       content_type, description, uploaded_by_id, uploaded_by, is_archived, row_version,
       created_at, updated_at
FROM attachments;
DROP TABLE attachments;
ALTER TABLE attachments_new RENAME TO attachments;
CREATE UNIQUE INDEX ux_attachments_path ON attachments (storage_folder, relative_path COLLATE NOCASE);
CREATE INDEX ix_attachments_asset ON attachments (asset_id);
CREATE INDEX ix_attachments_calibration ON attachments (calibration_id);

COMMIT;

PRAGMA foreign_keys = ON;
