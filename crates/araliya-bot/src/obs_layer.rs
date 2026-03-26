//! Tracing bridge — a [`tracing_subscriber::Layer`] that re-emits tracing
//! events as [`ObsEvent`](araliya_core::obs::ObsEvent) on the observability bus.
//!
//! This module lives in the binary crate (not `araliya-core`) so the core
//! stays dependency-light — only the binary needs the `registry` feature of
//! `tracing-subscriber`.
//!
//! # Sync safety
//!
//! `tracing::Layer::on_event` is synchronous and called on the logging
//! thread. `broadcast::Sender::send` is also synchronous and non-blocking
//! (it drops the oldest event if the buffer is full). No async bridge is
//! needed.
//!
//! # Performance
//!
//! The layer builds a `serde_json::Value` from the event's fields on every
//! call. The `min_level` filter short-circuits before any allocation for
//! events below threshold.

use std::sync::Arc;

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

use araliya_core::obs::{ObsBus, ObsEvent, ObsLevel};

/// A tracing layer that forwards events to the observability bus.
///
/// Created in `main.rs` and composed into the subscriber via
/// `tracing_subscriber::registry().with(fmt).with(obs_layer)`.
pub struct ObsTracingLayer {
    tx: Arc<tokio::sync::broadcast::Sender<ObsEvent>>,
    min_level: ObsLevel,
}

impl ObsTracingLayer {
    /// Create a new layer that emits events at or above `min_level` onto `bus`.
    pub fn new(bus: &ObsBus, min_level: ObsLevel) -> Self {
        Self {
            tx: Arc::clone(bus.sender()),
            min_level,
        }
    }
}

impl<S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>> Layer<S>
    for ObsTracingLayer
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();
        let level = ObsLevel::from_tracing(meta.level());

        // Short-circuit: skip events below threshold before allocating.
        if !level.is_at_least(self.min_level) {
            return;
        }

        // Collect fields from the event.
        let mut visitor = FieldCollector::default();
        event.record(&mut visitor);

        // Extract the message (tracing stores it as the "message" field).
        let message = visitor.message.take().unwrap_or_default();

        // Build the fields map (excluding the message itself).
        let fields = if visitor.fields.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::Object(visitor.fields)
        };

        // Extract span ID from the current span, if any.
        let span_id = ctx
            .lookup_current()
            .map(|span| format!("{:x}", span.id().into_u64()));

        let obs_event = ObsEvent {
            level,
            target: meta.target().to_string(),
            message,
            fields,
            session_id: None, // tracing bridge doesn't carry session context
            request_id: None,
            span_id,
            ts_unix_ms: unix_ms_now(),
        };

        // Non-blocking send — drops oldest on overflow, returns Err if no subscribers.
        let _ = self.tx.send(obs_event);
    }
}

/// Visitor that collects tracing event fields into a `serde_json::Map`.
#[derive(Default)]
struct FieldCollector {
    message: Option<String>,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl Visit for FieldCollector {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.fields.insert(
                field.name().to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let s = format!("{value:?}");
        if field.name() == "message" {
            self.message = Some(s);
        } else {
            self.fields
                .insert(field.name().to_string(), serde_json::Value::String(s));
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        if let Some(n) = serde_json::Number::from_f64(value) {
            self.fields
                .insert(field.name().to_string(), serde_json::Value::Number(n));
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), serde_json::Value::Bool(value));
    }
}

fn unix_ms_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
