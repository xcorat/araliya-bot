//! Observability event types.
//!
//! [`ObsEvent`] is the single structured event that flows through the
//! observability bus. It carries enough context to correlate across
//! subsystems (session, request, span IDs) while remaining cheap to clone
//! (`broadcast` requires `Clone`).

use serde::{Deserialize, Serialize};

/// Severity level for an observability event.
///
/// Mirrors tracing levels but is serde-friendly and decoupled from
/// the `tracing` crate so consumers don't need a tracing dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ObsLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl ObsLevel {
    /// Convert from a [`tracing::Level`].
    pub fn from_tracing(level: &tracing::Level) -> Self {
        match *level {
            tracing::Level::ERROR => Self::Error,
            tracing::Level::WARN => Self::Warn,
            tracing::Level::INFO => Self::Info,
            tracing::Level::DEBUG => Self::Debug,
            tracing::Level::TRACE => Self::Trace,
        }
    }

    /// Returns `true` if this level is at least as severe as `threshold`.
    ///
    /// Ordering: Error > Warn > Info > Debug > Trace.
    pub fn is_at_least(&self, threshold: ObsLevel) -> bool {
        *self >= threshold
    }
}

impl std::fmt::Display for ObsLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        })
    }
}

/// A single structured observability event.
///
/// Emitted by subsystems (via [`super::ObservabilityHandle`]) or bridged
/// from `tracing` macros (via [`super::layer::ObsTracingLayer`]).
///
/// Correlation IDs (`session_id`, `request_id`, `span_id`) are optional —
/// the tracing bridge populates `span_id` from the current tracing span,
/// while subsystem-emitted events carry richer context like `session_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsEvent {
    /// Severity level.
    pub level: ObsLevel,
    /// Source module or subsystem (e.g. `"araliya_agents::chat"` or `"llm"`).
    pub target: String,
    /// Human-readable event description.
    pub message: String,
    /// Arbitrary structured fields (e.g. `{"agent_id":"chat","tokens":150}`).
    #[serde(default)]
    pub fields: serde_json::Value,
    /// Session correlation ID (populated by subsystems, not the tracing bridge).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Bus request correlation ID (UUID from `BusMessage::Request.id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Tracing span ID (hex string from `span.id().into_u64()`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
    /// Wall-clock timestamp (milliseconds since Unix epoch).
    pub ts_unix_ms: u64,
}

impl ObsEvent {
    /// Create an event timestamped to now.
    pub fn now(
        level: ObsLevel,
        target: impl Into<String>,
        message: impl Into<String>,
        session_id: Option<String>,
        request_id: Option<String>,
        span_id: Option<String>,
    ) -> Self {
        Self {
            level,
            target: target.into(),
            message: message.into(),
            fields: serde_json::Value::Null,
            session_id,
            request_id,
            span_id,
            ts_unix_ms: unix_ms_now(),
        }
    }

    /// Attach structured fields to this event.
    pub fn with_fields(mut self, fields: serde_json::Value) -> Self {
        self.fields = fields;
        self
    }
}

/// Current wall-clock time as milliseconds since Unix epoch.
fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_ordering() {
        assert!(ObsLevel::Error > ObsLevel::Warn);
        assert!(ObsLevel::Warn > ObsLevel::Info);
        assert!(ObsLevel::Info > ObsLevel::Debug);
        assert!(ObsLevel::Debug > ObsLevel::Trace);
    }

    #[test]
    fn is_at_least() {
        assert!(ObsLevel::Error.is_at_least(ObsLevel::Warn));
        assert!(ObsLevel::Info.is_at_least(ObsLevel::Info));
        assert!(!ObsLevel::Debug.is_at_least(ObsLevel::Info));
    }

    #[test]
    fn event_now_populates_timestamp() {
        let event = ObsEvent::now(ObsLevel::Info, "test", "hello", None, None, None);
        assert!(event.ts_unix_ms > 0);
        assert_eq!(event.level, ObsLevel::Info);
        assert_eq!(event.target, "test");
        assert_eq!(event.message, "hello");
    }

    #[test]
    fn event_serializes_to_json() {
        let event = ObsEvent::now(ObsLevel::Warn, "llm", "timeout", None, None, None)
            .with_fields(serde_json::json!({"latency_ms": 5000}));
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"WARN\""));
        assert!(json.contains("\"latency_ms\":5000"));
    }

    #[test]
    fn level_display() {
        assert_eq!(ObsLevel::Info.to_string(), "INFO");
        assert_eq!(ObsLevel::Trace.to_string(), "TRACE");
    }
}
