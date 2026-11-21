-- Calibration extensions: authorize an item for use for a number of days past its due
-- date. An extension record has no performer, so "exactly one performer" becomes "at
-- most one"; the store requires one for every other kind of service.
ALTER TABLE service_types ADD is_extension BIT NOT NULL
    CONSTRAINT df_service_types_is_extension DEFAULT 0;
ALTER TABLE calibration_records ADD extension_days BIGINT NULL;
ALTER TABLE assets ADD cal_extended BIT NOT NULL
    CONSTRAINT df_assets_cal_extended DEFAULT 0;
GO
INSERT INTO service_types (name, description, counts_as_calibration, is_extension, sort_order, created_at, updated_at)
VALUES ('Calibration Extension', 'Authorizes use for a number of days past the due date; the item is not recalibrated.',
        0, 1, 30, SYSUTCDATETIME(), SYSUTCDATETIME());
ALTER TABLE calibration_records DROP CONSTRAINT ck_calibration_records_calibrated_by;
ALTER TABLE calibration_records ADD CONSTRAINT ck_calibration_records_performer
    CHECK (NOT (provider_id IS NOT NULL AND manufacturer_id IS NOT NULL));
