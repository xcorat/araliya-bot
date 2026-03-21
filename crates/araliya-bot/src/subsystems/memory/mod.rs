//! Memory subsystem — shim re-exporting from `araliya_memory`.
//!
//! All implementation now lives in the `araliya-memory` crate.
//! This module re-exports everything so that `use crate::subsystems::memory::*`
//! continues to work throughout `araliya-bot`.

pub use araliya_memory::*;
