//! Error types shared across Hexplace crates.

use std::fmt;

/// Recoverable failure in domain or engine operations.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// The requested place identifier was not found.
    #[error("place not found: {0}")]
    NotFound(u64),
    /// Input failed validation.
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    /// Underlying storage or index failure.
    #[error("storage error: {0}")]
    Storage(String),
    /// Search index failure.
    #[error("index error: {0}")]
    Index(String),
    /// Import or data directory failure.
    #[error("import error: {0}")]
    Import(String),
    /// I/O failure with context.
    #[error("io error: {0}")]
    Io(String),
}

impl CoreError {
    /// Builds an invalid-request error from displayable context.
    pub fn invalid(msg: impl fmt::Display) -> Self {
        Self::InvalidRequest(msg.to_string())
    }

    /// Builds a storage error from displayable context.
    pub fn storage(msg: impl fmt::Display) -> Self {
        Self::Storage(msg.to_string())
    }

    /// Builds an index error from displayable context.
    pub fn index(msg: impl fmt::Display) -> Self {
        Self::Index(msg.to_string())
    }

    /// Builds an import error from displayable context.
    pub fn import(msg: impl fmt::Display) -> Self {
        Self::Import(msg.to_string())
    }

    /// Builds an I/O error from displayable context.
    pub fn io(msg: impl fmt::Display) -> Self {
        Self::Io(msg.to_string())
    }
}

impl From<std::io::Error> for CoreError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}
