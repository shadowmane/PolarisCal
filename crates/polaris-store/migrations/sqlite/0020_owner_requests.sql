-- Check-out requests: someone who can't check items out (a Viewer) asks for an item to be
-- checked out or transferred to an owner; a Technician or Administrator approves (which
-- makes the change) or declines it.
CREATE TABLE owner_requests (
    id              INTEGER PRIMARY KEY,
    asset_id        INTEGER NOT NULL REFERENCES assets (id),
    kind            TEXT    NOT NULL CHECK (kind IN ('check_out', 'transfer')),
    owner_id        INTEGER NOT NULL REFERENCES owners (id),
    requested_by_id INTEGER REFERENCES users (id),
    requested_by    TEXT    NOT NULL,
    note            TEXT,
    status          TEXT    NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'declined', 'cancelled')),
    decided_by_id   INTEGER REFERENCES users (id),
    decided_by      TEXT,
    decided_at      TEXT,
    decision_note   TEXT,
    created_at      TEXT    NOT NULL,
    updated_at      TEXT    NOT NULL
);
CREATE INDEX ix_owner_requests_asset_id ON owner_requests (asset_id);
CREATE INDEX ix_owner_requests_status ON owner_requests (status);
