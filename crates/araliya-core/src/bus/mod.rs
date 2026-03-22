//! Supervisor bus protocol — typed message protocol, dispatch traits, health, and component info.
//!
//! This module defines the "language" of the bus. The actual dispatch loop
//! (supervisor) lives in a separate crate.

pub mod component;
pub mod dispatch;
pub mod handle;
pub mod health;
pub mod message;

// Re-export key types at `bus::` level for convenience.
pub use component::{ComponentInfo, ComponentStatus, ComponentStatusResponse};
pub use dispatch::BusHandler;
pub use handle::{BusCallError, BusHandle, SupervisorBus};
pub use health::{HealthRegistry, HealthReporter, SubsystemHealth};
pub use message::{
    BusError, BusMessage, BusPayload, BusResult, CronEntryInfo, CronScheduleSpec,
    ERR_METHOD_NOT_FOUND, StreamReceiver,
};
