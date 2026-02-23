// Library root — exposes internals for integration tests and future crate consumers.
// The binary entry point is src/main.rs.
#![allow(dead_code, unused_imports, unused_variables)]

pub mod error;
pub mod config;
pub mod llm;

#[cfg(feature = "subsystem-memory")]
pub mod subsystems {
    pub mod memory;
}
