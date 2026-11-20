-- PolarisCal schema for MySQL 8 / MariaDB 10.5 or later.
-- Equivalent to the SQLite schema through version 5, so it starts at version 5.
-- Conventions: dates are DATE, timestamps are UTC, nothing is hard-deleted
-- (rows are archived or deactivated), and row_version supports optimistic concurrency.
-- Text that must be unique is compared case-insensitively.

-- Admin-managed pick lists
CREATE TABLE locations (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    description MEDIUMTEXT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_locations_name ON locations (name);

CREATE TABLE departments (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    description MEDIUMTEXT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_departments_name ON departments (name);

CREATE TABLE owners (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    description MEDIUMTEXT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_owners_name ON owners (name);

CREATE TABLE manufacturers (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    description MEDIUMTEXT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_manufacturers_name ON manufacturers (name);

CREATE TABLE calibration_providers (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    description MEDIUMTEXT NULL,
    is_internal BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_calibration_providers_name ON calibration_providers (name);

CREATE TABLE asset_statuses (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(200) NOT NULL,
    description MEDIUMTEXT NULL,
    tracks_due BOOLEAN NOT NULL DEFAULT TRUE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order BIGINT NOT NULL DEFAULT 0,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_asset_statuses_name ON asset_statuses (name);

-- Users
CREATE TABLE users (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(100) NOT NULL,
    display_name MEDIUMTEXT NOT NULL,
    email MEDIUMTEXT NULL,
    role VARCHAR(20) NOT NULL,
    password_hash MEDIUMTEXT NOT NULL,
    must_change_password BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    last_sign_in_at DATETIME(6) NULL,
    row_version BIGINT NOT NULL DEFAULT 1,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    CHECK (role IN ('admin', 'technician', 'viewer'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_users_username ON users (username);

-- Instruments and Systems
CREATE TABLE assets (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    asset_tag VARCHAR(100) NOT NULL,
    kind VARCHAR(20) NOT NULL,
    parent_id BIGINT NULL,
    description MEDIUMTEXT NOT NULL,
    manufacturer_id BIGINT NULL,
    model_number MEDIUMTEXT NULL,
    serial_number MEDIUMTEXT NULL,
    location_id BIGINT NULL,
    department_id BIGINT NULL,
    owner_id BIGINT NULL,
    status_id BIGINT NOT NULL,
    warranty_end_date DATE NULL,
    cal_interval_value BIGINT NULL,
    cal_interval_unit VARCHAR(10) NULL,
    in_service_date DATE NULL,
    first_cal_date DATE NULL,
    last_cal_date DATE NULL,
    next_cal_date DATE NULL,
    notes MEDIUMTEXT NULL,
    is_archived BOOLEAN NOT NULL DEFAULT FALSE,
    row_version BIGINT NOT NULL DEFAULT 1,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    last_cal_date_entered DATE NULL,
    CONSTRAINT fk_assets_parent_id FOREIGN KEY (parent_id) REFERENCES assets (id),
    CONSTRAINT fk_assets_manufacturer_id FOREIGN KEY (manufacturer_id) REFERENCES manufacturers (id),
    CONSTRAINT fk_assets_location_id FOREIGN KEY (location_id) REFERENCES locations (id),
    CONSTRAINT fk_assets_department_id FOREIGN KEY (department_id) REFERENCES departments (id),
    CONSTRAINT fk_assets_owner_id FOREIGN KEY (owner_id) REFERENCES owners (id),
    CONSTRAINT fk_assets_status_id FOREIGN KEY (status_id) REFERENCES asset_statuses (id),
    CHECK (kind IN ('instrument', 'system')),
    CHECK (cal_interval_value IS NULL OR cal_interval_value > 0),
    CHECK (cal_interval_unit IN ('days', 'months')),
    CHECK ((cal_interval_value IS NULL AND cal_interval_unit IS NULL) OR (cal_interval_value IS NOT NULL AND cal_interval_unit IS NOT NULL))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_assets_asset_tag ON assets (asset_tag);
CREATE INDEX ix_assets_parent_id ON assets (parent_id);
CREATE INDEX ix_assets_next_cal_date ON assets (next_cal_date);
CREATE INDEX ix_assets_status_id ON assets (status_id);

-- Calibration records
CREATE TABLE calibration_records (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    asset_id BIGINT NOT NULL,
    cal_date DATE NOT NULL,
    provider_id BIGINT NOT NULL,
    technician MEDIUMTEXT NULL,
    cost_cents BIGINT NULL,
    certificate_number MEDIUMTEXT NULL,
    condition_received VARCHAR(30) NOT NULL,
    condition_returned VARCHAR(30) NOT NULL,
    next_due_date DATE NULL,
    notes MEDIUMTEXT NULL,
    is_archived BOOLEAN NOT NULL DEFAULT FALSE,
    row_version BIGINT NOT NULL DEFAULT 1,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    CONSTRAINT fk_calibration_records_asset_id FOREIGN KEY (asset_id) REFERENCES assets (id),
    CONSTRAINT fk_calibration_records_provider_id FOREIGN KEY (provider_id) REFERENCES calibration_providers (id),
    CHECK (cost_cents IS NULL OR cost_cents >= 0),
    CHECK (condition_received IN ('pass', 'out_of_tolerance', 'inoperative')),
    CHECK (condition_returned IN ('pass', 'fail', 'limited'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE INDEX ix_calibration_records_asset_id_cal_date ON calibration_records (asset_id, cal_date);

-- The audit trail. Append-only: rows are never altered or removed (see the triggers below).
CREATE TABLE audit_log (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    occurred_at DATETIME(6) NOT NULL,
    user_id BIGINT NULL,
    username VARCHAR(100) NOT NULL,
    action VARCHAR(40) NOT NULL,
    entity VARCHAR(40) NOT NULL,
    entity_id BIGINT NULL,
    asset_id BIGINT NULL,
    entity_label MEDIUMTEXT NULL,
    reason MEDIUMTEXT NULL,
    changes MEDIUMTEXT NOT NULL,
    CONSTRAINT fk_audit_log_user_id FOREIGN KEY (user_id) REFERENCES users (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE INDEX ix_audit_log_asset_id_id ON audit_log (asset_id, id);
CREATE INDEX ix_audit_log_entity_entity_id_id ON audit_log (entity, entity_id, id);
CREATE INDEX ix_audit_log_user_id_id ON audit_log (user_id, id);

-- Organization settings
CREATE TABLE app_settings (
    setting_key VARCHAR(100) NOT NULL PRIMARY KEY,
    setting_value MEDIUMTEXT NOT NULL,
    updated_at DATETIME(6) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

-- Files attached to Instruments/Systems and calibration records (paths are relative to the attachments folder)
CREATE TABLE attachments (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    asset_id BIGINT NOT NULL,
    calibration_id BIGINT NULL,
    category VARCHAR(20) NOT NULL,
    file_name MEDIUMTEXT NOT NULL,
    relative_path VARCHAR(700) NOT NULL,
    size_bytes BIGINT NOT NULL,
    sha256 VARCHAR(64) NOT NULL,
    content_type MEDIUMTEXT NULL,
    description MEDIUMTEXT NULL,
    uploaded_by_id BIGINT NULL,
    uploaded_by VARCHAR(100) NOT NULL,
    is_archived BOOLEAN NOT NULL DEFAULT FALSE,
    row_version BIGINT NOT NULL DEFAULT 1,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    CONSTRAINT fk_attachments_asset_id FOREIGN KEY (asset_id) REFERENCES assets (id),
    CONSTRAINT fk_attachments_calibration_id FOREIGN KEY (calibration_id) REFERENCES calibration_records (id),
    CONSTRAINT fk_attachments_uploaded_by_id FOREIGN KEY (uploaded_by_id) REFERENCES users (id),
    CHECK (category IN ('certificate', 'document', 'photo', 'other')),
    CHECK (size_bytes >= 0),
    CHECK (CHAR_LENGTH(sha256) = 64)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE UNIQUE INDEX ux_attachments_relative_path ON attachments (relative_path);
CREATE INDEX ix_attachments_asset_id ON attachments (asset_id);
CREATE INDEX ix_attachments_calibration_id ON attachments (calibration_id);

-- Default statuses
INSERT INTO asset_statuses (name, description, tracks_due, sort_order, created_at, updated_at) VALUES
    ('Active', 'In service and on a calibration schedule.', TRUE, 10, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6)),
    ('Out for Calibration', 'Currently away for calibration.', TRUE, 20, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6)),
    ('Out of Service', 'Not in use; excluded from due tracking.', FALSE, 30, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6)),
    ('Calibration Not Required', 'In use but does not require calibration.', FALSE, 40, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6)),
    ('Retired', 'Permanently removed from service.', FALSE, 50, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6)),
    ('Lost / Missing', 'Cannot be located.', FALSE, 60, UTC_TIMESTAMP(6), UTC_TIMESTAMP(6));

INSERT INTO app_settings (setting_key, setting_value, updated_at) VALUES ('idle_lock_minutes', '15', UTC_TIMESTAMP(6));

-- The audit trail is append-only.
CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log FOR EACH ROW
    SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'the audit log is append-only';
CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log FOR EACH ROW
    SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT = 'the audit log is append-only';
