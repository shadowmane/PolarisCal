-- Repairs: service types get an "is a repair" flag (types already named like a repair get it),
-- and an item remembers the status it had before a service changed it automatically (e.g.
-- before a failed calibration sent it to Out for Repair), offered back when a repair passes.
ALTER TABLE service_types ADD COLUMN is_repair INTEGER NOT NULL DEFAULT 0
    CHECK (is_repair IN (0, 1));
ALTER TABLE assets ADD COLUMN previous_status_id INTEGER REFERENCES asset_statuses (id);
UPDATE service_types SET is_repair = 1
 WHERE LOWER(name) LIKE '%repair%' AND is_extension = 0;
