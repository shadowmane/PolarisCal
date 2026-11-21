-- A calibration can be done by a calibration provider or by the item's manufacturer.
ALTER TABLE calibration_records DROP FOREIGN KEY fk_calibration_records_provider_id;
ALTER TABLE calibration_records MODIFY provider_id BIGINT NULL;
ALTER TABLE calibration_records
    ADD CONSTRAINT fk_calibration_records_provider_id
        FOREIGN KEY (provider_id) REFERENCES calibration_providers (id),
    ADD COLUMN manufacturer_id BIGINT NULL,
    ADD CONSTRAINT fk_calibration_records_manufacturer_id
        FOREIGN KEY (manufacturer_id) REFERENCES manufacturers (id);
ALTER TABLE calibration_records ADD CONSTRAINT ck_calibration_records_calibrated_by
    CHECK ((provider_id IS NULL AND manufacturer_id IS NOT NULL)
        OR (provider_id IS NOT NULL AND manufacturer_id IS NULL));
