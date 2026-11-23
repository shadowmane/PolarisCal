-- no-transaction
-- Storage: a built-in status for items moved out of active use but not retired. Stored
-- items aren't tracked for due dates; their next calibration date stays as it was, so
-- when one goes back into service it's due (or overdue) on its original schedule.
-- A status already named Storage becomes the built-in one.
PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE asset_statuses_new (
    id           INTEGER PRIMARY KEY,
    name         TEXT    NOT NULL,
    description  TEXT,
    tracks_due   INTEGER NOT NULL DEFAULT 1 CHECK (tracks_due IN (0, 1)),
    is_active    INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    sort_order   INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT    NOT NULL,
    updated_at   TEXT    NOT NULL,
    builtin_role TEXT    NULL
        CHECK (builtin_role IS NULL OR builtin_role IN ('out_for_calibration', 'out_for_repair', 'retired', 'storage'))
);
INSERT INTO asset_statuses_new (id, name, description, tracks_due, is_active, sort_order,
                                created_at, updated_at, builtin_role)
SELECT id, name, description, tracks_due, is_active, sort_order, created_at, updated_at, builtin_role
FROM asset_statuses;
DROP TABLE asset_statuses;
ALTER TABLE asset_statuses_new RENAME TO asset_statuses;
CREATE UNIQUE INDEX ux_asset_statuses_name ON asset_statuses (name COLLATE NOCASE);

UPDATE asset_statuses SET builtin_role = 'storage'
 WHERE LOWER(name) = 'storage' AND builtin_role IS NULL;
INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at, builtin_role)
SELECT 'Storage', 'Moved out of active use; not calibrated while stored. Its schedule resumes when it returns to service.', 0, 40, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'storage'
WHERE NOT EXISTS (SELECT 1 FROM asset_statuses WHERE builtin_role = 'storage')
  AND NOT EXISTS (SELECT 1 FROM asset_statuses WHERE LOWER(name) = 'storage');
UPDATE asset_statuses SET tracks_due = 0 WHERE builtin_role = 'storage';

COMMIT;

PRAGMA foreign_keys = ON;
