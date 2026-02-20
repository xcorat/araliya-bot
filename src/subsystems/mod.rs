//! Subsystem modules for the Araliya bot.

pub mod agents;
pub mod comms;
pub mod llm;
pub mod management;
#[cfg(feature = "subsystem-memory")]
pub mod memory;
pub mod runtime;
#[cfg(feature = "subsystem-tools")]
pub mod tools;
#[cfg(feature = "subsystem-ui")]
pub mod ui;
