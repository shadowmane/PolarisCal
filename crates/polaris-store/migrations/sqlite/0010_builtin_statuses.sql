-- Built-in statuses: a role marks the statuses the app relies on, so they keep working
-- if renamed. Out for Calibration and Out for Repair always track due dates (an item
-- away for service still shows when it's overdue); Retired never does. A failed
-- calibration sends an item to Out for Repair or Retired.
ALTER TABLE asset_statuses ADD COLUMN builtin_role TEXT NULL
    CHECK (builtin_role IS NULL OR builtin_role IN ('out_for_calibration', 'out_for_repair', 'retired'));
UPDATE asset_statuses SET builtin_role = 'out_for_calibration' WHERE LOWER(name) = 'out for calibration';
UPDATE asset_statuses SET builtin_role = 'out_for_repair' WHERE LOWER(name) = 'out for repair';
UPDATE asset_statuses SET builtin_role = 'retired' WHERE LOWER(name) = 'retired';
INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at, builtin_role)
SELECT 'Out for Calibration', 'Currently away for calibration.', 1, 20, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'out_for_calibration'
WHERE NOT EXISTS (SELECT 1 FROM asset_statuses WHERE builtin_role = 'out_for_calibration');
INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at, builtin_role)
SELECT 'Out for Repair', 'Away for repair, e.g. after failing calibration.', 1, 25, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'out_for_repair'
WHERE NOT EXISTS (SELECT 1 FROM asset_statuses WHERE builtin_role = 'out_for_repair');
INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at, builtin_role)
SELECT 'Retired', 'Permanently removed from service.', 0, 50, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), 'retired'
WHERE NOT EXISTS (SELECT 1 FROM asset_statuses WHERE builtin_role = 'retired');
UPDATE asset_statuses SET tracks_due = 1 WHERE builtin_role IN ('out_for_calibration', 'out_for_repair');
UPDATE asset_statuses SET tracks_due = 0 WHERE builtin_role = 'retired';
