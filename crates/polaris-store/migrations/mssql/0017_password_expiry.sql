-- Password expiry: when each password was last set. Existing passwords count from the
-- upgrade, so turning expiry on doesn't expire everyone's at once.
ALTER TABLE users ADD password_changed_at DATETIME2(7) NULL;
GO
UPDATE users SET password_changed_at = SYSUTCDATETIME();
