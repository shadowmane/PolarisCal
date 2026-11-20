//! Users, password rules, and password hashing (Argon2id).

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::Role;
use crate::{Error, FieldError, Id, Result};

pub const MIN_PASSWORD_LEN: usize = 10;
pub const MAX_PASSWORD_LEN: usize = 256;

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
    pub fn normalize(&mut self) -> Result<()> {
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
        if let Err(Error::Validation(mut e)) = check_password_rules(&self.username, &self.password)
        {
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

/// The password policy: at least [`MIN_PASSWORD_LEN`] characters and not the username.
pub fn check_password_rules(username: &str, password: &str) -> Result<()> {
    let len = password.chars().count();
    if len < MIN_PASSWORD_LEN {
        return Err(Error::validation(
            "password",
            format!("Password must be at least {MIN_PASSWORD_LEN} characters."),
        ));
    }
    if len > MAX_PASSWORD_LEN {
        return Err(Error::validation("password", "Password is too long."));
    }
    if password.trim().eq_ignore_ascii_case(username.trim()) {
        return Err(Error::validation(
            "password",
            "Password can't be the same as the username.",
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
        assert!(check_password_rules("jsmith", "short").is_err());
        assert!(check_password_rules("jsmith1234", "JSMITH1234").is_err());
        assert!(check_password_rules("jsmith", "long enough pass").is_ok());
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
        let Err(Error::Validation(errors)) = u.normalize() else {
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
