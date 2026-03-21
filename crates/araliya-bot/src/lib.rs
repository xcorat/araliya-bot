// Library root — exposes internals for integration tests and future crate consumers.
// The binary entry point is src/main.rs.
//
// Phase 1 of multi-crate split: shared foundation types now live in `araliya-core`.
// This module re-exports them so downstream code (`use crate::config`, etc.) still works.

// Re-export araliya-core modules under the same paths the codebase expects.
pub use araliya_core::config;
pub use araliya_core::error;

pub mod llm;

#[cfg(feature = "subsystem-memory")]
pub mod subsystems {
    pub mod memory;
}
