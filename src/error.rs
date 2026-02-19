//! Application-wide error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("config error: {0}")]
    Config(String),

    #[error("identity error: {0}")]
    Identity(String),

    #[error("logger error: {0}")]
    Logger(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn config_error_display() {
        let e = AppError::Config("missing field".into());
        assert!(!e.to_string().is_empty());
        assert!(e.to_string().contains("missing field"));
    }

    #[test]
    fn identity_error_display() {
        let e = AppError::Identity("key not found".into());
        assert!(e.to_string().contains("key not found"));
    }

    #[test]
    fn logger_error_display() {
        let e = AppError::Logger("already initialized".into());
        assert!(e.to_string().contains("already initialized"));
    }

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let e: AppError = io_err.into();
        assert!(e.to_string().contains("io error"));
        // satisfies std::error::Error trait
        let _: &dyn Error = &e;
    }
}
