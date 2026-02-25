//! Bootstrap layer — modules that run before subsystems start.
//!
//! - **identity** — ed25519 keypair generation and persistence.
//! - **logger** — tracing-subscriber initialisation.

pub mod identity;
pub mod logger;
