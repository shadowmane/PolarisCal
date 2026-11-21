-- Built-in statuses: a role marks the statuses the app relies on, so they keep working
-- if renamed. Out for Calibration and Out for Repair always track due dates (an item
-- away for service still shows when it's overdue); Retired never does. A failed
-- calibration sends an item to Out for Repair or Retired.
ALTER TABLE asset_statuses ADD COLUMN builtin_role VARCHAR(30) NULL,
    ADD CONSTRAINT ck_asset_statuses_builtin_role CHECK (builtin_role IS NULL OR builtin_role IN ('out_for_calibration', 'out_for_repair', 'retired'));
UPDATE asset_statuses SET builtin_role = 'out_for_calibration' WHERE LOWER(name) = 'out for calibration';
UPDATE asset_statuses SET builtin_role = 'out_for_repair' WHERE LOWER(name) = 'out for repair';
UPDATE asset_statuses SET builtin_role = 'retired' WHERE LOWER(name) = 'retired';
INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at, builtin_role)
SELECT 'Out for Calibration', 'Currently away for calibration.', TRUE, 20, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6), 'out_for_calibration' FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM asset_statuses WHERE builtin_role = 'out_for_calibration');
INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at, builtin_role)
SELECT 'Out for Repair', 'Away for repair, e.g. after failing calibration.', TRUE, 25, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6), 'out_for_repair' FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM asset_statuses WHERE builtin_role = 'out_for_repair');
INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at, builtin_role)
SELECT 'Retired', 'Permanently removed from service.', FALSE, 50, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6), 'retired' FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM asset_statuses WHERE builtin_role = 'retired');
UPDATE asset_statuses SET tracks_due = TRUE WHERE builtin_role IN ('out_for_calibration', 'out_for_repair');
UPDATE asset_statuses SET tracks_due = FALSE WHERE builtin_role = 'retired';
