// Library root — exposes internals for integration tests and future crate consumers.
// The binary entry point is src/main.rs.
//
// Phase 9 of multi-crate split: all shim modules removed; direct crate imports used.

pub use araliya_core::config;
pub use araliya_core::error;
