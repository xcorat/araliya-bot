//! Cron subsystem — timer-based event scheduling.

pub mod dispatcher;
pub(crate) mod service;

pub use dispatcher::CronSubsystem;
