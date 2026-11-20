-- PolarisCal initial schema (SQLite).
-- Conventions shared by every backend:
--   * dates are ISO-8601 'YYYY-MM-DD'; timestamps are RFC 3339 UTC text
--   * booleans are 0/1 integers
--   * nothing is hard-deleted: rows are archived (is_archived) or deactivated (is_active)
--   * row_version supports optimistic concurrency for multi-user editing

-- ---------------------------------------------------------------------------
-- Admin-managed pick lists
-- ---------------------------------------------------------------------------

CREATE TABLE locations (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    description TEXT,
    is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_locations_name ON locations (name COLLATE NOCASE);

CREATE TABLE departments (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    description TEXT,
    is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_departments_name ON departments (name COLLATE NOCASE);

CREATE TABLE owners (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    description TEXT,
    is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_owners_name ON owners (name COLLATE NOCASE);

CREATE TABLE manufacturers (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    description TEXT,
    is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_manufacturers_name ON manufacturers (name COLLATE NOCASE);

-- Who performs calibrations: internal technicians and outside vendors.
CREATE TABLE calibration_providers (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    description TEXT,
    is_internal INTEGER NOT NULL DEFAULT 0 CHECK (is_internal IN (0, 1)),
    is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_calibration_providers_name ON calibration_providers (name COLLATE NOCASE);

-- tracks_due = 0 keeps items in that status out of overdue/due-soon tracking.
CREATE TABLE asset_statuses (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    description TEXT,
    tracks_due  INTEGER NOT NULL DEFAULT 1 CHECK (tracks_due IN (0, 1)),
    is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL,
    updated_at  TEXT    NOT NULL
);
CREATE UNIQUE INDEX ux_asset_statuses_name ON asset_statuses (name COLLATE NOCASE);

-- ---------------------------------------------------------------------------
-- Instruments and Systems
-- ---------------------------------------------------------------------------

CREATE TABLE assets (
    id                 INTEGER PRIMARY KEY,
    asset_tag          TEXT    NOT NULL,
    kind               TEXT    NOT NULL CHECK (kind IN ('instrument', 'system')),
    parent_id          INTEGER REFERENCES assets (id),
    description        TEXT    NOT NULL,
    manufacturer_id    INTEGER REFERENCES manufacturers (id),
    model_number       TEXT,
    serial_number      TEXT,
    location_id        INTEGER REFERENCES locations (id),
    department_id      INTEGER REFERENCES departments (id),
    owner_id           INTEGER REFERENCES owners (id),
    status_id          INTEGER NOT NULL REFERENCES asset_statuses (id),
    warranty_end_date  TEXT,
    cal_interval_value INTEGER CHECK (cal_interval_value IS NULL OR cal_interval_value > 0),
    cal_interval_unit  TEXT    CHECK (cal_interval_unit IN ('days', 'months')),
    in_service_date    TEXT,
    first_cal_date     TEXT,
    last_cal_date      TEXT,
    next_cal_date      TEXT,
    notes              TEXT,
    is_archived        INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    row_version        INTEGER NOT NULL DEFAULT 1,
    created_at         TEXT    NOT NULL,
    updated_at         TEXT    NOT NULL,
    CHECK ((cal_interval_value IS NULL) = (cal_interval_unit IS NULL)),
    CHECK (parent_id IS NULL OR parent_id <> id)
);
CREATE UNIQUE INDEX ux_assets_tag ON assets (asset_tag COLLATE NOCASE);
CREATE INDEX ix_assets_parent ON assets (parent_id);
CREATE INDEX ix_assets_next_cal ON assets (next_cal_date);
CREATE INDEX ix_assets_status ON assets (status_id);

-- ---------------------------------------------------------------------------
-- Calibration records
-- ---------------------------------------------------------------------------

CREATE TABLE calibration_records (
    id                 INTEGER PRIMARY KEY,
    asset_id           INTEGER NOT NULL REFERENCES assets (id),
    cal_date           TEXT    NOT NULL,
    provider_id        INTEGER NOT NULL REFERENCES calibration_providers (id),
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
    updated_at         TEXT    NOT NULL
);
CREATE INDEX ix_calibration_records_asset ON calibration_records (asset_id, cal_date);
CREATE INDEX ix_calibration_records_cert ON calibration_records (certificate_number);

-- ---------------------------------------------------------------------------
-- Seed data
-- ---------------------------------------------------------------------------

INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at) VALUES
    ('Active',                    'In service and on a calibration schedule.',        1, 10, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ('Out for Calibration',       'Currently away for calibration.',                  1, 20, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ('Out of Service',            'Not in use; excluded from due tracking.',          0, 30, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ('Calibration Not Required',  'In use but does not require calibration.',         0, 40, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ('Retired',                   'Permanently removed from service.',                0, 50, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    ('Lost / Missing',            'Cannot be located.',                               0, 60, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
