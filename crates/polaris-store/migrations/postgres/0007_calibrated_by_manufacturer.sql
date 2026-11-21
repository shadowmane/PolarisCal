-- A calibration can be done by a calibration provider or by the item's manufacturer.
ALTER TABLE calibration_records ALTER COLUMN provider_id DROP NOT NULL;
ALTER TABLE calibration_records ADD COLUMN manufacturer_id BIGINT NULL;
ALTER TABLE calibration_records ADD CONSTRAINT fk_calibration_records_manufacturer_id
    FOREIGN KEY (manufacturer_id) REFERENCES manufacturers (id);
ALTER TABLE calibration_records ADD CONSTRAINT ck_calibration_records_calibrated_by
    CHECK ((provider_id IS NULL) <> (manufacturer_id IS NULL));
CREATE INDEX ix_calibration_records_manufacturer ON calibration_records (manufacturer_id);
