//! Supervisor bus protocol — typed message protocol, dispatch traits, health, and component info.
//!
//! This module defines the "language" of the bus. The actual dispatch loop
//! (supervisor) lives in a separate crate.

pub mod message;
pub mod handle;
pub mod dispatch;
pub mod health;
pub mod component;

// Re-export key types at `bus::` level for convenience.
pub use message::{BusMessage, BusPayload, BusResult, BusError, CronScheduleSpec, CronEntryInfo, StreamReceiver, ERR_METHOD_NOT_FOUND};
pub use handle::{BusHandle, BusCallError, SupervisorBus};
pub use dispatch::BusHandler;
pub use health::{HealthRegistry, HealthReporter, SubsystemHealth};
pub use component::{ComponentInfo, ComponentStatus, ComponentStatusResponse};
