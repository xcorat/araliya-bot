//! Subsystem modules for the Araliya bot.

pub mod llm;
#[cfg(feature = "subsystem-runtimes")]
pub mod runtimes;
#[cfg(feature = "subsystem-ui")]
pub mod ui;
