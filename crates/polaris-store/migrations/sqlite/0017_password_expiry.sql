-- Password expiry: when each password was last set. Existing passwords count from the
-- upgrade, so turning expiry on doesn't expire everyone's at once.
ALTER TABLE users ADD COLUMN password_changed_at TEXT;
UPDATE users SET password_changed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now');
