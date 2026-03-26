//! Observability bus — broadcast-based pub/sub for [`ObsEvent`].
//!
//! The [`ObsBus`] wraps a `tokio::sync::broadcast` channel. Subsystems
//! receive an [`ObservabilityHandle`] (cloneable emit surface) and call
//! `.emit()` to publish events. Consumers call `obs_bus.subscribe()` to
//! receive a `broadcast::Receiver<ObsEvent>`.
//!
//! # Back-pressure
//!
//! `broadcast` is bounded and drops the oldest message when full (the
//! subscriber sees `RecvError::Lagged`). This is intentional — observability
//! must never block the hot path.

use std::sync::Arc;

use tokio::sync::broadcast;

use super::event::{ObsEvent, ObsLevel};

/// Default capacity for the broadcast channel (power-of-two, ring semantics).
const OBS_BUS_CAPACITY: usize = 512;

/// The observability bus — create once at startup, clone and distribute.
///
/// Internally holds an `Arc<broadcast::Sender<ObsEvent>>` so cloning is
/// cheap (reference count bump).
#[derive(Clone)]
pub struct ObsBus {
    tx: Arc<broadcast::Sender<ObsEvent>>,
}

impl ObsBus {
    /// Create a new bus with the default capacity (512 events).
    pub fn new() -> Self {
        Self::with_capacity(OBS_BUS_CAPACITY)
    }

    /// Create a new bus with a specific capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx: Arc::new(tx) }
    }

    /// Create an [`ObservabilityHandle`] for emitting events onto this bus.
    pub fn handle(&self) -> ObservabilityHandle {
        ObservabilityHandle {
            tx: Arc::clone(&self.tx),
        }
    }

    /// Subscribe to receive all future events.
    ///
    /// Each subscriber gets its own cursor into the ring buffer. If a
    /// subscriber falls behind, `recv()` returns `RecvError::Lagged(n)` —
    /// the subscriber should log the gap and continue.
    pub fn subscribe(&self) -> broadcast::Receiver<ObsEvent> {
        self.tx.subscribe()
    }

    /// Number of active subscribers (useful for diagnostics).
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Get a reference to the inner sender (used by the obs tracing layer).
    pub fn sender(&self) -> &Arc<broadcast::Sender<ObsEvent>> {
        &self.tx
    }
}

impl Default for ObsBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Cloneable handle for emitting events onto the [`ObsBus`].
///
/// Subsystems receive this via constructor injection (same pattern as
/// [`crate::bus::BusHandle`] and [`crate::bus::HealthReporter`]).
#[derive(Clone)]
pub struct ObservabilityHandle {
    tx: Arc<broadcast::Sender<ObsEvent>>,
}

impl ObservabilityHandle {
    /// Emit a pre-built event. Returns `false` if there are no subscribers.
    pub fn emit(&self, event: ObsEvent) -> bool {
        self.tx.send(event).is_ok()
    }

    /// Emit an info-level event with no correlation IDs.
    pub fn info(&self, target: &str, message: impl Into<String>) {
        self.emit(ObsEvent::now(
            ObsLevel::Info,
            target,
            message,
            None,
            None,
            None,
        ));
    }

    /// Emit a warn-level event with no correlation IDs.
    pub fn warn(&self, target: &str, message: impl Into<String>) {
        self.emit(ObsEvent::now(
            ObsLevel::Warn,
            target,
            message,
            None,
            None,
            None,
        ));
    }

    /// Emit an error-level event with no correlation IDs.
    pub fn error(&self, target: &str, message: impl Into<String>) {
        self.emit(ObsEvent::now(
            ObsLevel::Error,
            target,
            message,
            None,
            None,
            None,
        ));
    }

    /// Emit a debug-level event with no correlation IDs.
    pub fn debug(&self, target: &str, message: impl Into<String>) {
        self.emit(ObsEvent::now(
            ObsLevel::Debug,
            target,
            message,
            None,
            None,
            None,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_received_by_subscriber() {
        let bus = ObsBus::new();
        let mut rx = bus.subscribe();
        let handle = bus.handle();

        handle.info("test", "hello");

        let event = rx.recv().await.unwrap();
        assert_eq!(event.target, "test");
        assert_eq!(event.message, "hello");
        assert_eq!(event.level, ObsLevel::Info);
    }

    #[tokio::test]
    async fn multiple_subscribers_each_receive() {
        let bus = ObsBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        let handle = bus.handle();

        handle.warn("llm", "slow");

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.message, "slow");
        assert_eq!(e2.message, "slow");
    }

    #[tokio::test]
    async fn no_subscribers_emit_returns_false() {
        let bus = ObsBus::new();
        let handle = bus.handle();
        // No subscribers — emit should return false.
        assert!(!handle.emit(ObsEvent::now(
            ObsLevel::Info,
            "test",
            "nobody listening",
            None,
            None,
            None
        )));
    }

    #[test]
    fn subscriber_count() {
        let bus = ObsBus::new();
        assert_eq!(bus.subscriber_count(), 0);
        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);
        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);
    }

    #[tokio::test]
    async fn lagged_subscriber_sees_error() {
        // Tiny capacity to force lag.
        let bus = ObsBus::with_capacity(2);
        let mut rx = bus.subscribe();
        let handle = bus.handle();

        // Send 4 events into a capacity-2 buffer.
        for i in 0..4 {
            handle.info("test", format!("msg-{i}"));
        }

        // First recv should report lagged.
        match rx.recv().await {
            Err(broadcast::error::RecvError::Lagged(_)) => {} // expected
            other => panic!("expected Lagged, got {other:?}"),
        }
    }
}
