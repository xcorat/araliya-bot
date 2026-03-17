//! Subsystem modules for the Araliya bot.

#[cfg(feature = "subsystem-agents")]
pub mod agents;
pub mod comms;
#[cfg(feature = "subsystem-cron")]
pub mod cron;
pub mod llm;
pub mod management;
#[cfg(feature = "subsystem-memory")]
pub mod memory;
#[cfg(all(feature = "subsystem-memory", feature = "subsystem-agents"))]
pub mod memory_bus;
pub mod runtime;
#[cfg(feature = "subsystem-runtimes")]
pub mod runtimes;
#[cfg(feature = "subsystem-tools")]
pub mod tools;
#[cfg(feature = "subsystem-ui")]
pub mod ui;
