-- Keep the last calibration date a user typed in (for items brought over from
-- another system) separate from the derived last_cal_date. Without it, archiving
-- every calibration record of an item would leave a record's date behind.
ALTER TABLE assets ADD COLUMN last_cal_date_entered TEXT;

-- Existing items with no active records: their last date can only have been entered.
UPDATE assets
SET last_cal_date_entered = last_cal_date
WHERE NOT EXISTS (
    SELECT 1 FROM calibration_records r WHERE r.asset_id = assets.id AND r.is_archived = 0
);
