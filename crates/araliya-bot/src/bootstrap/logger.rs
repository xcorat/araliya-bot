//! Logging initialisation via tracing-subscriber.
//!
//! Call [`init`] once at startup, after runtime settings are resolved.

use std::path::Path;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::EnvFilter;

use crate::error::AppError;

/// Initialise the global tracing subscriber.
///
/// `level` accepts standard level strings: `"error"`, `"warn"`, `"info"`,
/// `"debug"`, `"trace"`.
///
/// If `prefer_level` is `true`, `level` takes precedence and `RUST_LOG` is only
/// used as a fallback when `level` is invalid. If `prefer_level` is `false`,
/// `RUST_LOG` takes precedence and `level` is the fallback.
pub fn init(level: &str, prefer_level: bool, log_file: Option<&Path>) -> Result<(), AppError> {
    let filter = if prefer_level {
        match EnvFilter::try_new(level) {
            Ok(filter) => filter,
            Err(level_err) => EnvFilter::try_from_default_env()
                .map_err(|env_err| {
                    AppError::Logger(format!(
                        "invalid log level '{level}': {level_err}; RUST_LOG parse failed: {env_err}"
                    ))
                })?,
        }
    } else {
        EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(level))
            .map_err(|e| AppError::Logger(format!("invalid log level '{level}': {e}")))?
    };

    let writer = if let Some(path) = log_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| {
                AppError::Logger(format!(
                    "failed to open log file '{}': {e}",
                    path.display()
                ))
            })?;
        BoxMakeWriter::new(file)
    } else {
        BoxMakeWriter::new(std::io::stderr)
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .try_init()
        .map_err(|e| AppError::Logger(format!("failed to set subscriber: {e}")))?;

    Ok(())
}

/// Parse a log level string into a [`LevelFilter`], returning an error on
/// unrecognised values. Useful for validating config before re-initialising.
pub fn parse_level(level: &str) -> Result<LevelFilter, AppError> {
    if level.is_empty() {
        return Err(AppError::Logger("log level must not be empty".into()));
    }
    level
        .parse::<LevelFilter>()
        .map_err(|_| AppError::Logger(format!("unrecognised log level: '{level}'")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_levels_parse() {
        for l in &["error", "warn", "info", "debug", "trace"] {
            assert!(parse_level(l).is_ok(), "expected '{l}' to be valid");
        }
    }

    #[test]
    fn invalid_level_errors() {
        assert!(parse_level("verbose").is_err());
        assert!(parse_level("").is_err());
        assert!(parse_level("INFO_LEVEL").is_err());
    }

    #[test]
    fn init_info_succeeds_or_already_init() {
        // May already be set by a prior test run in the same process — both outcomes are fine.
        let result = init("info", false, None);
        match result {
            Ok(()) => {}
            Err(AppError::Logger(msg)) if msg.contains("set subscriber") => {}
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
