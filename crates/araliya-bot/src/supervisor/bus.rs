//! Supervisor event bus — re-exported from `araliya-core::bus`.
//!
//! This shim preserves `use crate::supervisor::bus::*` import paths across the
//! codebase while the canonical types live in `araliya-core`.

pub use araliya_core::bus::message::{
    BusMessage, BusPayload, BusResult, BusError, CronScheduleSpec, CronEntryInfo, StreamReceiver,
    ERR_METHOD_NOT_FOUND,
};
pub use araliya_core::bus::handle::{BusHandle, BusCallError, SupervisorBus};

// Re-export StreamChunk at this level (was `pub use crate::llm::providers::openai_compatible::StreamChunk`)
pub use araliya_core::types::llm::StreamChunk;
