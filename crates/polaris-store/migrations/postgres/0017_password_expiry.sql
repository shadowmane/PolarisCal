-- Password expiry: when each password was last set. Existing passwords count from the
-- upgrade, so turning expiry on doesn't expire everyone's at once.
ALTER TABLE users ADD COLUMN password_changed_at TIMESTAMPTZ NULL;
UPDATE users SET password_changed_at = now();
