//! PolarisCal core: the domain model, business rules, and the [`Store`] trait
//! that every database backend (SQLite, PostgreSQL, MySQL, SQL Server) implements.
//!
//! Nothing in this crate touches a database or the UI. Rules that every backend
//! must apply identically (date derivation, validation, due status) live here so
//! that backends only translate them into SQL.

pub mod audit;
pub mod auth;
pub mod error;
pub mod model;
pub mod schedule;
pub mod store;

pub use error::{Error, FieldError, Result};
pub use store::Store;

/// Database row identifier. All tables use 64-bit integer surrogate keys.
pub type Id = i64;
