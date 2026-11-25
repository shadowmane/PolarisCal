-- Check-out requests: someone who can't check items out (a Viewer) asks for an item to be
-- checked out or transferred to an owner; a Technician or Administrator approves (which
-- makes the change) or declines it.
CREATE TABLE owner_requests (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    asset_id BIGINT NOT NULL,
    kind VARCHAR(20) NOT NULL,
    owner_id BIGINT NOT NULL,
    requested_by_id BIGINT NULL,
    requested_by VARCHAR(100) NOT NULL,
    note MEDIUMTEXT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    decided_by_id BIGINT NULL,
    decided_by VARCHAR(100) NULL,
    decided_at DATETIME(6) NULL,
    decision_note MEDIUMTEXT NULL,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    CONSTRAINT fk_owner_requests_asset_id FOREIGN KEY (asset_id) REFERENCES assets (id),
    CONSTRAINT fk_owner_requests_owner_id FOREIGN KEY (owner_id) REFERENCES owners (id),
    CONSTRAINT fk_owner_requests_requested_by_id FOREIGN KEY (requested_by_id) REFERENCES users (id),
    CONSTRAINT fk_owner_requests_decided_by_id FOREIGN KEY (decided_by_id) REFERENCES users (id),
    CONSTRAINT ck_owner_requests_kind CHECK (kind IN ('check_out', 'transfer')),
    CONSTRAINT ck_owner_requests_status CHECK (status IN ('pending', 'approved', 'declined', 'cancelled'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE INDEX ix_owner_requests_asset_id ON owner_requests (asset_id);
CREATE INDEX ix_owner_requests_status ON owner_requests (status);
