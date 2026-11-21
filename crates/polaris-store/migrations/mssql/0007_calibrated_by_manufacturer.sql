-- A calibration can be done by a calibration provider or by the item's manufacturer.
ALTER TABLE calibration_records DROP CONSTRAINT fk_calibration_records_provider_id;
ALTER TABLE calibration_records ALTER COLUMN provider_id BIGINT NULL;
ALTER TABLE calibration_records ADD CONSTRAINT fk_calibration_records_provider_id
    FOREIGN KEY (provider_id) REFERENCES calibration_providers (id);
ALTER TABLE calibration_records ADD manufacturer_id BIGINT NULL
    CONSTRAINT fk_calibration_records_manufacturer_id REFERENCES manufacturers (id);
GO
ALTER TABLE calibration_records ADD CONSTRAINT ck_calibration_records_calibrated_by
    CHECK ((provider_id IS NULL AND manufacturer_id IS NOT NULL)
        OR (provider_id IS NOT NULL AND manufacturer_id IS NULL));
CREATE INDEX ix_calibration_records_manufacturer ON calibration_records (manufacturer_id);
