-- Storage: a built-in status for items moved out of active use but not retired. Stored
-- items aren't tracked for due dates; their next calibration date stays as it was, so
-- when one goes back into service it's due (or overdue) on its original schedule.
-- A status already named Storage becomes the built-in one.
ALTER TABLE asset_statuses DROP CONSTRAINT ck_asset_statuses_builtin_role;
ALTER TABLE asset_statuses ADD CONSTRAINT ck_asset_statuses_builtin_role
    CHECK (builtin_role IS NULL OR builtin_role IN ('out_for_calibration', 'out_for_repair', 'retired', 'storage'));
UPDATE asset_statuses SET builtin_role = 'storage'
 WHERE LOWER(name) = 'storage' AND builtin_role IS NULL;
INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at, builtin_role)
SELECT 'Storage', 'Moved out of active use; not calibrated while stored. Its schedule resumes when it returns to service.', FALSE, 40, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6), 'storage' FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM asset_statuses WHERE builtin_role = 'storage')
  AND NOT EXISTS (SELECT 1 FROM asset_statuses WHERE LOWER(name) = 'storage');
UPDATE asset_statuses SET tracks_due = FALSE WHERE builtin_role = 'storage';
