-- Service records have a type (Calibration, Repair, ...). Only types that count as a
-- calibration set an item's last calibration and next due dates.
CREATE TABLE service_types (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    description MEDIUMTEXT NULL,
    counts_as_calibration BOOLEAN NOT NULL DEFAULT TRUE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_service_types_name ON service_types (name);
INSERT INTO service_types (name, description, counts_as_calibration, sort_order, created_at, updated_at) VALUES
    ('Calibration', 'Calibrated against a standard; sets the next due date.', TRUE, 10, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6)),
    ('Repair', 'Repaired or adjusted; does not change the calibration schedule.', FALSE, 20, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6));

-- Every existing record was a calibration.
ALTER TABLE calibration_records ADD COLUMN service_type_id BIGINT NULL;
UPDATE calibration_records SET service_type_id = (SELECT id FROM service_types WHERE name = 'Calibration');
ALTER TABLE calibration_records
    MODIFY service_type_id BIGINT NOT NULL,
    ADD CONSTRAINT fk_calibration_records_service_type_id
        FOREIGN KEY (service_type_id) REFERENCES service_types (id);
