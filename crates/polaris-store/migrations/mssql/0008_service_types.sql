-- Service records have a type (Calibration, Repair, ...). Only types that count as a
-- calibration set an item's last calibration and next due dates.
CREATE TABLE service_types (
    id BIGINT IDENTITY(1,1) NOT NULL PRIMARY KEY,
    name NVARCHAR(200) NOT NULL,
    description NVARCHAR(MAX) NULL,
    counts_as_calibration BIT NOT NULL DEFAULT 1,
    is_active BIT NOT NULL DEFAULT 1,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at DATETIME2(7) NOT NULL,
    updated_at DATETIME2(7) NOT NULL
);
CREATE UNIQUE INDEX ux_service_types_name ON service_types (name);
INSERT INTO service_types (name, description, counts_as_calibration, sort_order, created_at, updated_at) VALUES
    ('Calibration', 'Calibrated against a standard; sets the next due date.', 1, 10, SYSUTCDATETIME(), SYSUTCDATETIME()),
    ('Repair', 'Repaired or adjusted; does not change the calibration schedule.', 0, 20, SYSUTCDATETIME(), SYSUTCDATETIME());

-- Every existing record was a calibration.
ALTER TABLE calibration_records ADD service_type_id BIGINT NULL;
GO
UPDATE calibration_records SET service_type_id = (SELECT id FROM service_types WHERE name = 'Calibration');
ALTER TABLE calibration_records ALTER COLUMN service_type_id BIGINT NOT NULL;
ALTER TABLE calibration_records ADD CONSTRAINT fk_calibration_records_service_type_id
    FOREIGN KEY (service_type_id) REFERENCES service_types (id);
CREATE INDEX ix_calibration_records_service_type ON calibration_records (service_type_id);
