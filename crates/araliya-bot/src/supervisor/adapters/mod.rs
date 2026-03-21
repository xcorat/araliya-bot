//! Supervisor transport adapters — re-exported from `araliya-supervisor`.

pub mod stdio;
#[cfg(unix)]
pub mod uds;

pub use araliya_supervisor::adapters::start;
