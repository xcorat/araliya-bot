//! Subsystem modules for the Araliya bot.

#[cfg(feature = "subsystem-agents")]
pub mod agents;
pub mod llm;
#[cfg(feature = "subsystem-runtimes")]
pub mod runtimes;
#[cfg(feature = "subsystem-ui")]
pub mod ui;
