//! Observability pub/sub — structured event bus for subsystem telemetry.
//!
//! This module provides the core types and broadcast bus for decoupling
//! event producers (subsystems) from consumers (management ring buffer,
//! SSE endpoints).
//!
//! ```text
//! subsystem emit ─► ObservabilityHandle ─► ObsBus (broadcast<ObsEvent>)
//!                                              │
//!                                        ┌─────┴────────────┐
//!                                  ManagementSub         SSE endpoint
//!                                  (ring buffer +        (/api/observe)
//!                                   bus notify)
//! ```
//!
//! - [`ObsBus`] — the broadcast channel, created once in `main.rs`.
//! - [`ObservabilityHandle`] — cloneable emit surface for subsystems.
//! - [`ObsEvent`] / [`ObsLevel`] — the structured event type.
//!
//! The tracing bridge (`ObsTracingLayer`) that forwards `tracing` macro
//! events into the bus lives in `araliya-bot` (binary crate), keeping
//! this module dependency-light.
//!
//! # Back-pressure
//!
//! The bus is bounded (default 512 events). When full, the oldest event
//! is dropped. Slow consumers see `RecvError::Lagged` and should re-sync
//! from the management ring buffer or simply skip the gap.

pub mod bus;
pub mod event;

pub use bus::{ObsBus, ObservabilityHandle};
pub use event::{ObsEvent, ObsLevel};
