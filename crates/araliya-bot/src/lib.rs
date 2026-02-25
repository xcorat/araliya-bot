// Library root — exposes internals for integration tests and future crate consumers.
// The binary entry point is src/main.rs.

mod core;
pub use core::{config, error};
pub mod llm;

#[cfg(feature = "subsystem-memory")]
pub mod subsystems {
    pub mod memory;
}
