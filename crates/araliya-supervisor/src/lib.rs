//! Supervisor runtime orchestrator.
//!
//! Contains the dispatch loop, internal control plane, transport adapters,
//! and the management bus handler. Depends on `araliya-core` for bus protocol
//! types, traits, and shared primitives.

pub mod control;
pub mod run;
pub mod adapters;
pub mod management;
