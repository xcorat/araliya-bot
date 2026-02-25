//! Health registry — push-based subsystem health state.
//!
//! Each subsystem holds a [`HealthReporter`] handle and writes its state
//! whenever it changes (startup, periodic check, error recovery).  The
//! [`HealthRegistry`] stores the last-written state per subsystem and returns
//! a snapshot on demand — no fan-out on read, no latency from subsystem I/O.
//!
//! # Pattern
//!
//! Subsystems push state: `reporter.set_healthy().await` or
//! `reporter.set_unhealthy("reason").await`.  The management subsystem reads
//! the registry snapshot: `registry.snapshot().await`.  Health endpoints are
//! always fast because they read cached state.
//!
//! For subsystems with external dependencies (e.g. LLM provider reachability),
//! spawn a background task that runs a lightweight check on a timer and calls
//! the reporter.  Other subsystems simply set healthy at startup and unhealthy
//! on errors.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

// ── SubsystemHealth ───────────────────────────────────────────────────────────

/// Health state snapshot for a single subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsystemHealth {
    /// Subsystem identifier (matches the [`BusHandler::prefix`]).
    pub id: String,
    /// `true` = healthy; `false` = unhealthy or degraded.
    pub healthy: bool,
    /// Human-readable status message.
    pub message: String,
    /// Optional structured extra fields (latency, counts, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl SubsystemHealth {
    pub fn ok(id: impl Into<String>) -> Self {
        Self { id: id.into(), healthy: true, message: "ok".into(), details: None }
    }

    pub fn degraded(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self { id: id.into(), healthy: false, message: message.into(), details: None }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

// ── HealthRegistry ────────────────────────────────────────────────────────────

/// Shared registry of per-subsystem health states.
///
/// Clone freely — it is backed by an `Arc` and is `Send + Sync`.
#[derive(Clone, Default)]
pub struct HealthRegistry {
    inner: Arc<RwLock<HashMap<String, SubsystemHealth>>>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a reporter handle for a subsystem.
    ///
    /// The reporter writes into this registry under the given `id`.
    pub fn reporter(&self, id: impl Into<String>) -> HealthReporter {
        HealthReporter { id: id.into(), registry: self.clone() }
    }

    /// Snapshot all current health states, sorted by id.
    pub async fn snapshot(&self) -> Vec<SubsystemHealth> {
        let map = self.inner.read().await;
        let mut v: Vec<_> = map.values().cloned().collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    /// `true` if every registered subsystem is healthy, or no subsystems are registered.
    pub async fn all_healthy(&self) -> bool {
        self.inner.read().await.values().all(|h| h.healthy)
    }
}

// ── HealthReporter ────────────────────────────────────────────────────────────

/// Per-subsystem write handle into the [`HealthRegistry`].
///
/// Clone freely — writes are serialised through the registry's `RwLock`.
#[derive(Clone)]
pub struct HealthReporter {
    id: String,
    registry: HealthRegistry,
}

impl HealthReporter {
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Mark the subsystem as healthy with a default "ok" message.
    pub async fn set_healthy(&self) {
        self.write(SubsystemHealth::ok(&self.id)).await;
    }

    /// Mark the subsystem as healthy with a custom message and optional details.
    pub async fn set_healthy_with(
        &self,
        message: impl Into<String>,
        details: Option<serde_json::Value>,
    ) {
        let mut h = SubsystemHealth::ok(&self.id);
        h.message = message.into();
        h.details = details;
        self.write(h).await;
    }

    /// Mark the subsystem as unhealthy with a reason message.
    pub async fn set_unhealthy(&self, message: impl Into<String>) {
        self.write(SubsystemHealth::degraded(&self.id, message)).await;
    }

    /// Mark as unhealthy with a reason and optional structured details.
    pub async fn set_unhealthy_with(
        &self,
        message: impl Into<String>,
        details: Option<serde_json::Value>,
    ) {
        let mut h = SubsystemHealth::degraded(&self.id, message);
        h.details = details;
        self.write(h).await;
    }

    /// Read the current health state for this subsystem from the registry.
    ///
    /// Returns `None` if no state has been written yet (subsystem has not
    /// reported health since startup).
    pub async fn get_current(&self) -> Option<SubsystemHealth> {
        self.registry.inner.read().await.get(&self.id).cloned()
    }

    async fn write(&self, h: SubsystemHealth) {
        self.registry.inner.write().await.insert(self.id.clone(), h);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reporter_set_healthy_reflects_in_snapshot() {
        let registry = HealthRegistry::new();
        let reporter = registry.reporter("llm");

        reporter.set_healthy().await;

        let snapshot = registry.snapshot().await;
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].id, "llm");
        assert!(snapshot[0].healthy);
        assert_eq!(snapshot[0].message, "ok");
    }

    #[tokio::test]
    async fn reporter_set_unhealthy_marks_degraded() {
        let registry = HealthRegistry::new();
        let reporter = registry.reporter("llm");

        reporter.set_unhealthy("connection refused").await;

        let snapshot = registry.snapshot().await;
        assert!(!snapshot[0].healthy);
        assert_eq!(snapshot[0].message, "connection refused");
    }

    #[tokio::test]
    async fn all_healthy_true_when_all_ok() {
        let registry = HealthRegistry::new();
        registry.reporter("llm").set_healthy().await;
        registry.reporter("agents").set_healthy().await;

        assert!(registry.all_healthy().await);
    }

    #[tokio::test]
    async fn all_healthy_false_when_one_degraded() {
        let registry = HealthRegistry::new();
        registry.reporter("llm").set_healthy().await;
        registry.reporter("agents").set_unhealthy("agents down").await;

        assert!(!registry.all_healthy().await);
    }

    #[tokio::test]
    async fn snapshot_sorted_by_id() {
        let registry = HealthRegistry::new();
        registry.reporter("tools").set_healthy().await;
        registry.reporter("agents").set_healthy().await;
        registry.reporter("cron").set_healthy().await;

        let ids: Vec<_> = registry.snapshot().await.into_iter().map(|h| h.id).collect();
        assert_eq!(ids, vec!["agents", "cron", "tools"]);
    }

    #[tokio::test]
    async fn get_current_returns_none_before_first_write() {
        let registry = HealthRegistry::new();
        let reporter = registry.reporter("unset");

        assert!(reporter.get_current().await.is_none());
    }

    #[tokio::test]
    async fn get_current_returns_latest_state() {
        let registry = HealthRegistry::new();
        let reporter = registry.reporter("llm");

        reporter.set_healthy().await;
        assert!(reporter.get_current().await.unwrap().healthy);

        reporter.set_unhealthy("timeout").await;
        let current = reporter.get_current().await.unwrap();
        assert!(!current.healthy);
        assert_eq!(current.message, "timeout");
    }

    #[tokio::test]
    async fn multiple_reporters_same_registry_independent() {
        let registry = HealthRegistry::new();
        let r1 = registry.reporter("llm");
        let r2 = registry.reporter("agents");

        r1.set_unhealthy("offline").await;
        r2.set_healthy().await;

        assert!(!r1.get_current().await.unwrap().healthy);
        assert!(r2.get_current().await.unwrap().healthy);
        // Registry sees both, not all-healthy.
        assert!(!registry.all_healthy().await);
    }

    #[tokio::test]
    async fn cloned_reporter_writes_to_same_registry() {
        let registry = HealthRegistry::new();
        let reporter = registry.reporter("llm");
        let cloned = reporter.clone();

        cloned.set_unhealthy("clone wrote this").await;

        let current = reporter.get_current().await.unwrap();
        assert_eq!(current.message, "clone wrote this");
    }

    #[tokio::test]
    async fn with_details_includes_extra_fields() {
        let registry = HealthRegistry::new();
        let reporter = registry.reporter("llm");

        reporter.set_healthy_with(
            "ok",
            Some(serde_json::json!({ "model": "gpt-4", "latency_ms": 120 })),
        ).await;

        let h = reporter.get_current().await.unwrap();
        assert!(h.healthy);
        let details = h.details.unwrap();
        assert_eq!(details["model"], "gpt-4");
        assert_eq!(details["latency_ms"], 120);
    }

    #[tokio::test]
    async fn empty_registry_all_healthy_is_true() {
        let registry = HealthRegistry::new();
        assert!(registry.all_healthy().await);
        assert!(registry.snapshot().await.is_empty());
    }
}
