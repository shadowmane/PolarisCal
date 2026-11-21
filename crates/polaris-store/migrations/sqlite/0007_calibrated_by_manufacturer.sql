-- no-transaction
-- A calibration can be done by a calibration provider or by the item's manufacturer.
-- SQLite can't make provider_id optional in place, so the table is rebuilt (the
-- documented 12-step procedure: foreign keys off, copy, swap, check, foreign keys on).
PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE calibration_records_new (
    id                 INTEGER PRIMARY KEY,
    asset_id           INTEGER NOT NULL REFERENCES assets (id),
    cal_date           TEXT    NOT NULL,
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
    id, asset_id, cal_date, provider_id, manufacturer_id, technician, cost_cents,
    certificate_number, condition_received, condition_returned, next_due_date, notes,
    is_archived, row_version, created_at, updated_at)
SELECT
    id, asset_id, cal_date, provider_id, NULL, technician, cost_cents,
    certificate_number, condition_received, condition_returned, next_due_date, notes,
    is_archived, row_version, created_at, updated_at
FROM calibration_records;

DROP TABLE calibration_records;
ALTER TABLE calibration_records_new RENAME TO calibration_records;

CREATE INDEX ix_calibration_records_asset ON calibration_records (asset_id, cal_date);
CREATE INDEX ix_calibration_records_cert ON calibration_records (certificate_number);
CREATE INDEX ix_calibration_records_manufacturer ON calibration_records (manufacturer_id);

COMMIT;

PRAGMA foreign_keys = ON;
