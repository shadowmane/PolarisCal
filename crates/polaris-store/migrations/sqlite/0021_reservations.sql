-- Reservations, and linking user accounts to Owner entries.
-- * owners.user_id: the user account an Owner entry is (set by an administrator). Requests
--   for items held by a linked owner go to that user.
-- * owner_requests.approver_user_id: the user who decides a request; empty means a
--   Technician or Administrator.
-- * reservations: an Instrument booked for a user for a span of days. 'pending' is a
--   reservation that waits on the holder of an earlier, overlapping reservation agreeing
--   to end theirs early (asks_reservation_id); 'active' ones are booked.
ALTER TABLE owners ADD COLUMN user_id INTEGER REFERENCES users (id);
CREATE UNIQUE INDEX ux_owners_user_id ON owners (user_id) WHERE user_id IS NOT NULL;
ALTER TABLE owner_requests ADD COLUMN approver_user_id INTEGER REFERENCES users (id);
CREATE TABLE reservations (
    id                  INTEGER PRIMARY KEY,
    asset_id            INTEGER NOT NULL REFERENCES assets (id),
    user_id             INTEGER NOT NULL REFERENCES users (id),
    start_date          TEXT    NOT NULL,
    end_date            TEXT    NOT NULL,
    note                TEXT,
    status              TEXT    NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'pending', 'declined', 'cancelled')),
    asks_reservation_id INTEGER REFERENCES reservations (id),
    decided_by          TEXT,
    decided_at          TEXT,
    decision_note       TEXT,
    created_at          TEXT    NOT NULL,
    updated_at          TEXT    NOT NULL,
    CHECK (end_date >= start_date)
);
CREATE INDEX ix_reservations_asset_id ON reservations (asset_id);
CREATE INDEX ix_reservations_user_id ON reservations (user_id);
