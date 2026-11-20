use serde::Serialize;
use ts_rs::TS;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A problem with one input field, reported back to the form that sent it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{entity} {id} was not found")]
    NotFound { entity: &'static str, id: i64 },

    #[error("{}", summarize(.0))]
    Validation(Vec<FieldError>),

    /// The record changed since the caller loaded it (optimistic concurrency).
    #[error("{0}")]
    Conflict(String),

    /// A uniqueness rule was violated, e.g. a duplicate asset tag.
    #[error("{0}")]
    Duplicate(String),

    #[error("database error: {0}")]
    Database(String),

    /// Settings or connection problems; the message is a complete sentence.
    #[error("{0}")]
    Config(String),
}

impl Error {
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation(vec![FieldError::new(field, message)])
    }

    pub fn stale(entity: &str) -> Self {
        Self::Conflict(format!(
            "This {entity} was changed by someone else after you opened it. Reload it and try again."
        ))
    }
}

fn summarize(errors: &[FieldError]) -> String {
    match errors {
        [] => "validation failed".into(),
        [one] => one.message.clone(),
        [first, rest @ ..] => format!("{} (and {} more problems)", first.message, rest.len()),
    }
}
