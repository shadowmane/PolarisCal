-- Calibration extensions: authorize an item for use for a number of days past its due
-- date. An extension record has no performer, so "exactly one performer" becomes "at
-- most one"; the store requires one for every other kind of service.
ALTER TABLE service_types ADD COLUMN is_extension BOOLEAN NOT NULL DEFAULT FALSE;
INSERT INTO service_types (name, description, counts_as_calibration, is_extension, sort_order, created_at, updated_at)
VALUES ('Calibration Extension', 'Authorizes use for a number of days past the due date; the item is not recalibrated.',
        FALSE, TRUE, 30, now(), now());

ALTER TABLE calibration_records ADD COLUMN extension_days BIGINT NULL;
ALTER TABLE calibration_records DROP CONSTRAINT ck_calibration_records_calibrated_by;
ALTER TABLE calibration_records ADD CONSTRAINT ck_calibration_records_performer
    CHECK (NOT (provider_id IS NOT NULL AND manufacturer_id IS NOT NULL));

ALTER TABLE assets ADD COLUMN cal_extended BOOLEAN NOT NULL DEFAULT FALSE;
