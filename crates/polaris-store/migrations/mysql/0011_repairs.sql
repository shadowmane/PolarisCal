-- Repairs: service types get an "is a repair" flag (types already named like a repair get it),
-- and an item remembers the status it had before a service changed it automatically (e.g.
-- before a failed calibration sent it to Out for Repair), offered back when a repair passes.
ALTER TABLE service_types ADD COLUMN is_repair BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE assets ADD COLUMN previous_status_id BIGINT NULL,
    ADD CONSTRAINT fk_assets_previous_status_id FOREIGN KEY (previous_status_id) REFERENCES asset_statuses (id);
UPDATE service_types SET is_repair = TRUE
 WHERE LOWER(name) LIKE '%repair%' AND is_extension = FALSE;
