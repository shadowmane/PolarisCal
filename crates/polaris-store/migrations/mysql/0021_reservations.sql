-- Reservations, and linking user accounts to Owner entries.
-- * owners.user_id: the user account an Owner entry is (set by an administrator). Requests
--   for items held by a linked owner go to that user.
-- * owner_requests.approver_user_id: the user who decides a request; empty means a
--   Technician or Administrator.
-- * reservations: an Instrument booked for a user for a span of days. 'pending' is a
--   reservation that waits on the holder of an earlier, overlapping reservation agreeing
--   to end theirs early (asks_reservation_id); 'active' ones are booked.
ALTER TABLE owners ADD COLUMN user_id BIGINT NULL;
ALTER TABLE owners ADD CONSTRAINT fk_owners_user_id FOREIGN KEY (user_id) REFERENCES users (id);
-- A unique index allows any number of NULLs here, so it only limits linked owners.
CREATE UNIQUE INDEX ux_owners_user_id ON owners (user_id);
ALTER TABLE owner_requests ADD COLUMN approver_user_id BIGINT NULL;
ALTER TABLE owner_requests ADD CONSTRAINT fk_owner_requests_approver_user_id FOREIGN KEY (approver_user_id) REFERENCES users (id);
CREATE TABLE reservations (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    asset_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    start_date DATE NOT NULL,
    end_date DATE NOT NULL,
    note MEDIUMTEXT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'active',
    asks_reservation_id BIGINT NULL,
    decided_by VARCHAR(100) NULL,
    decided_at DATETIME(6) NULL,
    decision_note MEDIUMTEXT NULL,
    created_at DATETIME(6) NOT NULL,
    updated_at DATETIME(6) NOT NULL,
    CONSTRAINT fk_reservations_asset_id FOREIGN KEY (asset_id) REFERENCES assets (id),
    CONSTRAINT fk_reservations_user_id FOREIGN KEY (user_id) REFERENCES users (id),
    CONSTRAINT fk_reservations_asks_reservation_id FOREIGN KEY (asks_reservation_id) REFERENCES reservations (id),
    CONSTRAINT ck_reservations_status CHECK (status IN ('active', 'pending', 'declined', 'cancelled')),
    CONSTRAINT ck_reservations_dates CHECK (end_date >= start_date)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
CREATE INDEX ix_reservations_asset_id ON reservations (asset_id);
CREATE INDEX ix_reservations_user_id ON reservations (user_id);
