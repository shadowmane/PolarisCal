-- no-transaction
-- Calibration extensions: authorize an item for use for a number of days past its due
-- date. An extension record has no performer, so "exactly one performer" becomes "at
-- most one" (the store requires one for every other kind of service). Changing a CHECK
-- means rebuilding the table (foreign keys off, copy, swap, foreign keys on).
PRAGMA foreign_keys = OFF;

BEGIN;

ALTER TABLE service_types ADD COLUMN is_extension INTEGER NOT NULL DEFAULT 0
    CHECK (is_extension IN (0, 1));
INSERT INTO service_types (name, description, counts_as_calibration, is_extension, sort_order, created_at, updated_at)
VALUES ('Calibration Extension', 'Authorizes use for a number of days past the due date; the item is not recalibrated.',
        0, 1, 30, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

ALTER TABLE assets ADD COLUMN cal_extended INTEGER NOT NULL DEFAULT 0 CHECK (cal_extended IN (0, 1));

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
    extension_days     INTEGER CHECK (extension_days IS NULL OR extension_days >= 1),
    is_archived        INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    row_version        INTEGER NOT NULL DEFAULT 1,
    created_at         TEXT    NOT NULL,
    updated_at         TEXT    NOT NULL,
    -- at most one of the two says who did the work (extensions have neither)
    CHECK (NOT (provider_id IS NOT NULL AND manufacturer_id IS NOT NULL))
);

INSERT INTO calibration_records_new (
    id, asset_id, cal_date, service_type_id, provider_id, manufacturer_id, technician,
    cost_cents, certificate_number, condition_received, condition_returned, next_due_date,
    notes, extension_days, is_archived, row_version, created_at, updated_at)
SELECT
    id, asset_id, cal_date, service_type_id, provider_id, manufacturer_id, technician,
    cost_cents, certificate_number, condition_received, condition_returned, next_due_date,
    notes, NULL, is_archived, row_version, created_at, updated_at
FROM calibration_records;

DROP TABLE calibration_records;
ALTER TABLE calibration_records_new RENAME TO calibration_records;

CREATE INDEX ix_calibration_records_asset ON calibration_records (asset_id, cal_date);
CREATE INDEX ix_calibration_records_cert ON calibration_records (certificate_number);
CREATE INDEX ix_calibration_records_manufacturer ON calibration_records (manufacturer_id);
CREATE INDEX ix_calibration_records_service_type ON calibration_records (service_type_id);

COMMIT;

PRAGMA foreign_keys = ON;
