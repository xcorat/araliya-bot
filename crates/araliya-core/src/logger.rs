//! Logging initialisation via tracing-subscriber.
//!
//! Call [`init`] once at startup for a standard fmt subscriber, or use
//! [`build_filter`] + [`build_writer`] to compose a custom layered
//! subscriber (e.g. with an observability layer in the binary crate).

use std::path::Path;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

use crate::error::AppError;

/// Build an [`EnvFilter`] from a level string and preference flag.
///
/// If `prefer_level` is `true`, `level` takes precedence and `RUST_LOG` is
/// the fallback. If `false`, `RUST_LOG` wins and `level` is the fallback.
pub fn build_filter(level: &str, prefer_level: bool) -> Result<EnvFilter, AppError> {
    if prefer_level {
        match EnvFilter::try_new(level) {
            Ok(filter) => Ok(filter),
            Err(level_err) => EnvFilter::try_from_default_env().map_err(|env_err| {
                AppError::Logger(format!(
                    "invalid log level '{level}': {level_err}; RUST_LOG parse failed: {env_err}"
                ))
            }),
        }
    } else {
        EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(level))
            .map_err(|e| AppError::Logger(format!("invalid log level '{level}': {e}")))
    }
}

/// Build a [`BoxMakeWriter`] targeting a log file (append mode) or stderr.
pub fn build_writer(log_file: Option<&Path>) -> Result<BoxMakeWriter, AppError> {
    if let Some(path) = log_file {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| {
                AppError::Logger(format!("failed to open log file '{}': {e}", path.display()))
            })?;
        Ok(BoxMakeWriter::new(file))
    } else {
        Ok(BoxMakeWriter::new(std::io::stderr))
    }
}

/// Initialise the global tracing subscriber with a standard fmt layer.
///
/// For composing additional layers (e.g. an observability bridge), use
/// [`build_filter`] + [`build_writer`] directly and call
/// `tracing_subscriber::registry().with(filter).with(fmt).with(extra).try_init()`.
pub fn init(level: &str, prefer_level: bool, log_file: Option<&Path>) -> Result<(), AppError> {
    let filter = build_filter(level, prefer_level)?;
    let writer = build_writer(log_file)?;

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

    #[test]
    fn build_filter_prefers_level_when_flagged() {
        let f = build_filter("debug", true);
        assert!(f.is_ok());
    }

    #[test]
    fn build_filter_fallback_when_not_flagged() {
        let f = build_filter("info", false);
        assert!(f.is_ok());
    }

    #[test]
    fn build_writer_stderr_default() {
        let w = build_writer(None);
        assert!(w.is_ok());
    }
}
