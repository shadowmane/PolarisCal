-- no-transaction
-- Service records have a type (Calibration, Repair, ...). Only types that count as a
-- calibration set an item's last calibration and next due dates. Every existing record
-- was a calibration. Adding a required column with a foreign key means rebuilding the
-- table (foreign keys off, copy, swap, foreign keys on).
PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE service_types (
    id                    INTEGER PRIMARY KEY,
    name                  TEXT    NOT NULL,
    description           TEXT,
    counts_as_calibration INTEGER NOT NULL DEFAULT 1 CHECK (counts_as_calibration IN (0, 1)),
    is_active             INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    sort_order            INTEGER NOT NULL DEFAULT 0,
    created_at            TEXT    NOT NULL,
    updated_at            TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_service_types_name ON service_types (name COLLATE NOCASE);
INSERT INTO service_types (name, description, counts_as_calibration, sort_order, created_at, updated_at) VALUES
    ('Calibration', 'Calibrated against a standard; sets the next due date.', 1, 10,
     strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ('Repair', 'Repaired or adjusted; does not change the calibration schedule.', 0, 20,
     strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

CREATE TABLE calibration_records_new (
    id                 INTEGER PRIMARY KEY,
    asset_id           INTEGER NOT NULL REFERENCES assets (id),
    cal_date           TEXT    NOT NULL,
    service_type_id    INTEGER NOT NULL REFERENCES service_types (id),
    provider_id        INTEGER REFERENCES calibration_providers (id),
    manufacturer_id    INTEGER REFERENCES manufacturers (id),
    technician         TEXT,
    cost_cents         INTEGER CHECK (cost_cents IS NULL OR cost_cents >= 0),
    certificate_number TEXT,
    condition_received TEXT    NOT NULL
        CHECK (condition_received IN ('pass', 'out_of_tolerance', 'inoperative')),
    condition_returned TEXT    NOT NULL
        CHECK (condition_returned IN ('pass', 'fail', 'limited')),
    next_due_date      TEXT,
    notes              TEXT,
    is_archived        INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    row_version        INTEGER NOT NULL DEFAULT 1,
    created_at         TEXT    NOT NULL,
    updated_at         TEXT    NOT NULL,
    -- exactly one of the two says who did the work
    CHECK ((provider_id IS NULL) <> (manufacturer_id IS NULL))
);

INSERT INTO calibration_records_new (
    id, asset_id, cal_date, service_type_id, provider_id, manufacturer_id, technician,
    cost_cents, certificate_number, condition_received, condition_returned, next_due_date,
    notes, is_archived, row_version, created_at, updated_at)
SELECT
    id, asset_id, cal_date, (SELECT id FROM service_types WHERE name = 'Calibration'),
    provider_id, manufacturer_id, technician,
    cost_cents, certificate_number, condition_received, condition_returned, next_due_date,
    notes, is_archived, row_version, created_at, updated_at
FROM calibration_records;

DROP TABLE calibration_records;
ALTER TABLE calibration_records_new RENAME TO calibration_records;

CREATE INDEX ix_calibration_records_asset ON calibration_records (asset_id, cal_date);
CREATE INDEX ix_calibration_records_cert ON calibration_records (certificate_number);
CREATE INDEX ix_calibration_records_manufacturer ON calibration_records (manufacturer_id);
CREATE INDEX ix_calibration_records_service_type ON calibration_records (service_type_id);

COMMIT;

PRAGMA foreign_keys = ON;
