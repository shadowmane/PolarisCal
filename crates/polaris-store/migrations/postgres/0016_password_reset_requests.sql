-- Password reset requests: someone who can't sign in can ask for a reset from the sign-in
-- screen. The request stays on the account until an administrator sets a new password.
ALTER TABLE users ADD COLUMN reset_requested_at TIMESTAMPTZ NULL;
