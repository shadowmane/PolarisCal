//! Users, password rules, and password hashing (Argon2id).

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::Role;
use crate::{Error, FieldError, Id, Result};

/// The default minimum length, used until an administrator sets another.
pub const MIN_PASSWORD_LEN: usize = 10;
pub const MAX_PASSWORD_LEN: usize = 256;

/// The rules new passwords must follow and how long they last, set by an administrator.
/// Existing passwords aren't checked against new rules; the next one is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PasswordPolicy {
    pub min_length: u32,
    pub require_uppercase: bool,
    pub require_number: bool,
    /// Anything that isn't a letter, a number, or a space, e.g. `!` or `#`.
    pub require_symbol: bool,
    /// Days a password stays valid before it must be changed at sign-in; `None` = never.
    pub expiry_days: Option<u32>,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: MIN_PASSWORD_LEN as u32,
            require_uppercase: false,
            require_number: false,
            require_symbol: false,
            expiry_days: None,
        }
    }
}

impl PasswordPolicy {
    pub const MIN_LENGTH_RANGE: std::ops::RangeInclusive<u32> = 8..=128;
    pub const EXPIRY_DAYS_RANGE: std::ops::RangeInclusive<u32> = 1..=3650;

    pub fn normalize(&self) -> Result<()> {
        if !Self::MIN_LENGTH_RANGE.contains(&self.min_length) {
            return Err(Error::validation(
                "password_min_length",
                format!(
                    "Minimum length must be between {} and {} characters.",
                    Self::MIN_LENGTH_RANGE.start(),
                    Self::MIN_LENGTH_RANGE.end()
                ),
            ));
        }
        if let Some(days) = self.expiry_days
            && !Self::EXPIRY_DAYS_RANGE.contains(&days)
        {
            return Err(Error::validation(
                "password_expiry_days",
                format!(
                    "Passwords must last between {} and {} days, or never expire.",
                    Self::EXPIRY_DAYS_RANGE.start(),
                    Self::EXPIRY_DAYS_RANGE.end()
                ),
            ));
        }
        Ok(())
    }

    /// Checks a new password: long enough, has what's required, and isn't the username.
    pub fn check(&self, username: &str, password: &str) -> Result<()> {
        let len = password.chars().count();
        if len < self.min_length as usize {
            return Err(Error::validation(
                "password",
                format!("Password must be at least {} characters.", self.min_length),
            ));
        }
        if len > MAX_PASSWORD_LEN {
            return Err(Error::validation("password", "Password is too long."));
        }
        let mut missing = Vec::new();
        if self.require_uppercase && !password.chars().any(char::is_uppercase) {
            missing.push("a capital letter");
        }
        if self.require_number && !password.chars().any(char::is_numeric) {
            missing.push("a number");
        }
        if self.require_symbol && !password.chars().any(is_symbol) {
            missing.push("a special character such as ! or #");
        }
        if !missing.is_empty() {
            return Err(Error::validation(
                "password",
                format!("Password must include {}.", join_and(&missing)),
            ));
        }
        if password.trim().eq_ignore_ascii_case(username.trim()) {
            return Err(Error::validation(
                "password",
                "Password can't be the same as the username.",
            ));
        }
        Ok(())
    }

    /// Whether a password set at `changed_at` has expired by `now`. Accounts with no date
    /// recorded don't expire.
    pub fn is_expired(&self, changed_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        match (self.expiry_days, changed_at) {
            (Some(days), Some(changed)) => now - changed >= Duration::days(days.into()),
            _ => false,
        }
    }
}

fn is_symbol(c: char) -> bool {
    !c.is_alphanumeric() && !c.is_whitespace()
}

/// "a", "a and b", "a, b, and c".
fn join_and(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_string(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// A user account, without its password hash.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct User {
    pub id: Id,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub role: Role,
    pub is_active: bool,
    /// Set when an admin creates the account or resets the password.
    pub must_change_password: bool,
    pub last_sign_in_at: Option<DateTime<Utc>>,
    /// When the current password was set (for expiry).
    pub password_changed_at: Option<DateTime<Utc>>,
    /// When the user asked for a password reset from the sign-in screen; cleared when a
    /// new password is set.
    pub reset_requested_at: Option<DateTime<Utc>>,
    /// The Owner entry this account is (linked by an administrator), if any.
    pub owner_id: Option<Id>,
    pub owner_name: Option<String>,
    pub row_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A user plus the stored hash, used only to check a sign-in. Never sent to the UI.
#[derive(Debug, Clone)]
pub struct UserCredentials {
    pub user: User,
    pub password_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NewUser {
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub role: Role,
    pub password: String,
    pub must_change_password: bool,
}

impl NewUser {
    /// Tidies the fields and checks them, the password against `policy`.
    pub fn normalize(&mut self, policy: &PasswordPolicy) -> Result<()> {
        let mut errors = Vec::new();
        self.username = self.username.trim().to_string();
        self.display_name = self.display_name.trim().to_string();
        self.email = self
            .email
            .take()
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty());

        if let Err(e) = check_username(&self.username) {
            errors.push(e);
        }
        if self.display_name.is_empty() {
            errors.push(FieldError::new("display_name", "Name is required."));
        }
        if let Err(Error::Validation(mut e)) = policy.check(&self.username, &self.password) {
            errors.append(&mut e);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::Validation(errors))
        }
    }
}

/// Fields an admin can change on an existing account. Passwords change separately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct UserUpdate {
    pub display_name: String,
    pub email: Option<String>,
    pub role: Role,
    pub is_active: bool,
}

impl UserUpdate {
    pub fn normalize(&mut self) -> Result<()> {
        self.display_name = self.display_name.trim().to_string();
        self.email = self
            .email
            .take()
            .map(|e| e.trim().to_string())
            .filter(|e| !e.is_empty());
        if self.display_name.is_empty() {
            return Err(Error::validation("display_name", "Name is required."));
        }
        Ok(())
    }
}

fn check_username(username: &str) -> std::result::Result<(), FieldError> {
    let len = username.chars().count();
    if !(3..=64).contains(&len) {
        return Err(FieldError::new(
            "username",
            "Username must be 3 to 64 characters.",
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'))
    {
        return Err(FieldError::new(
            "username",
            "Username can only contain letters, numbers, and . _ - @",
        ));
    }
    Ok(())
}

/// Hashes a password as an Argon2id PHC string with a random salt.
pub fn hash_password(password: &str) -> Result<String> {
    let hash: PasswordHash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| Error::Database(format!("could not hash password: {e}")))?;
    Ok(hash.to_string())
}

/// Checks a password against a stored PHC string. Malformed hashes never match.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_verify_and_are_salted() {
        let a = hash_password("correct horse battery").unwrap();
        let b = hash_password("correct horse battery").unwrap();
        assert!(a.starts_with("$argon2id$"));
        assert_ne!(a, b, "random salt per hash");
        assert!(verify_password("correct horse battery", &a));
        assert!(!verify_password("wrong password!!", &a));
        assert!(!verify_password("anything", "not-a-hash"));
    }

    #[test]
    fn password_rules() {
        let p = PasswordPolicy::default();
        assert!(p.check("jsmith", "short").is_err());
        assert!(p.check("jsmith1234", "JSMITH1234").is_err());
        assert!(p.check("jsmith", "long enough pass").is_ok());
    }

    #[test]
    fn stricter_password_rules_say_whats_missing() {
        let p = PasswordPolicy {
            min_length: 12,
            require_uppercase: true,
            require_number: true,
            require_symbol: true,
            expiry_days: None,
        };
        let message = |pw: &str| match p.check("jsmith", pw) {
            Err(Error::Validation(e)) => e[0].message.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            message("long enough"),
            "Password must be at least 12 characters."
        );
        assert_eq!(
            message("long enough pass"),
            "Password must include a capital letter, a number, and a special character such as ! or #."
        );
        assert_eq!(
            message("Long enough pass"),
            "Password must include a number and a special character such as ! or #."
        );
        // A space isn't a special character; punctuation is.
        assert!(p.check("jsmith", "Long enough 2026").is_err());
        assert!(p.check("jsmith", "Long-enough 2026").is_ok());
    }

    #[test]
    fn password_policy_limits_and_expiry() {
        let mut p = PasswordPolicy {
            min_length: 6,
            ..Default::default()
        };
        assert!(p.normalize().is_err());
        p.min_length = 8;
        p.expiry_days = Some(0);
        assert!(p.normalize().is_err());
        p.expiry_days = Some(90);
        assert!(p.normalize().is_ok());

        let now = Utc::now();
        assert!(!p.is_expired(Some(now - Duration::days(89)), now));
        assert!(p.is_expired(Some(now - Duration::days(90)), now));
        assert!(!p.is_expired(None, now));
        p.expiry_days = None;
        assert!(!p.is_expired(Some(now - Duration::days(9000)), now));
    }

    #[test]
    fn new_user_validation_collects_errors() {
        let mut u = NewUser {
            username: " a ".into(),
            display_name: " ".into(),
            email: Some("  ".into()),
            role: Role::Viewer,
            password: "x".into(),
            must_change_password: true,
        };
        let Err(Error::Validation(errors)) = u.normalize(&PasswordPolicy::default()) else {
            panic!()
        };
        let fields: Vec<_> = errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(fields, ["username", "display_name", "password"]);
        assert_eq!(u.email, None);
    }

    #[test]
    fn roles() {
        assert!(Role::Technician.can_edit_records());
        assert!(!Role::Viewer.can_edit_records());
        assert!(!Role::Technician.is_admin());
    }
}
