-- "key" and "value" are reserved words on MySQL and SQL Server. Rename them so
-- every backend shares one schema and the same queries.
ALTER TABLE app_settings RENAME COLUMN key TO setting_key;
ALTER TABLE app_settings RENAME COLUMN value TO setting_value;
