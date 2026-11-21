-- Repairs: service types get an "is a repair" flag (types already named like a repair get it),
-- and an item remembers the status it had before a service changed it automatically (e.g.
-- before a failed calibration sent it to Out for Repair), offered back when a repair passes.
ALTER TABLE service_types ADD is_repair BIT NOT NULL
    CONSTRAINT df_service_types_is_repair DEFAULT 0;
ALTER TABLE assets ADD previous_status_id BIGINT NULL
    CONSTRAINT fk_assets_previous_status_id REFERENCES asset_statuses (id);
GO
UPDATE service_types SET is_repair = 1
 WHERE LOWER(name) LIKE '%repair%' AND is_extension = 0;
