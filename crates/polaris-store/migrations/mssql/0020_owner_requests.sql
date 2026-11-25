-- Check-out requests: someone who can't check items out (a Viewer) asks for an item to be
-- checked out or transferred to an owner; a Technician or Administrator approves (which
-- makes the change) or declines it.
CREATE TABLE owner_requests (
    id BIGINT IDENTITY(1,1) NOT NULL PRIMARY KEY,
    asset_id BIGINT NOT NULL,
    kind NVARCHAR(20) NOT NULL,
    owner_id BIGINT NOT NULL,
    requested_by_id BIGINT NULL,
    requested_by NVARCHAR(100) NOT NULL,
    note NVARCHAR(MAX) NULL,
    status NVARCHAR(20) NOT NULL CONSTRAINT df_owner_requests_status DEFAULT 'pending',
    decided_by_id BIGINT NULL,
    decided_by NVARCHAR(100) NULL,
    decided_at DATETIME2(7) NULL,
    decision_note NVARCHAR(MAX) NULL,
    created_at DATETIME2(7) NOT NULL,
    updated_at DATETIME2(7) NOT NULL,
    CONSTRAINT fk_owner_requests_asset_id FOREIGN KEY (asset_id) REFERENCES assets (id),
    CONSTRAINT fk_owner_requests_owner_id FOREIGN KEY (owner_id) REFERENCES owners (id),
    CONSTRAINT fk_owner_requests_requested_by_id FOREIGN KEY (requested_by_id) REFERENCES users (id),
    CONSTRAINT fk_owner_requests_decided_by_id FOREIGN KEY (decided_by_id) REFERENCES users (id),
    CONSTRAINT ck_owner_requests_kind CHECK (kind IN ('check_out', 'transfer')),
    CONSTRAINT ck_owner_requests_status CHECK (status IN ('pending', 'approved', 'declined', 'cancelled'))
);
CREATE INDEX ix_owner_requests_asset_id ON owner_requests (asset_id);
CREATE INDEX ix_owner_requests_status ON owner_requests (status);
