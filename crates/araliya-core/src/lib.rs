//! Araliya core — shared foundation for all araliya crates.
//!
//! This crate provides:
//! - **config** — TOML-based configuration loading with env-var overrides
//! - **error** — application-wide error enum (`AppError`)
//! - **identity** — ed25519 keypair generation, persistence, and `public_id` derivation
//! - **logger** — tracing-subscriber initialisation
//! - **bus** — supervisor bus protocol types, dispatch traits, health registry
//! - **obs** — observability pub/sub bus (structured events, tracing bridge)
//! - **runtime** — generic subsystem component model
//! - **types** — shared types (LLM usage, timing, streaming chunks)

pub mod bus;
pub mod config;
pub mod error;
pub mod identity;
pub mod logger;
pub mod obs;
pub mod runtime;
pub mod types;
pub mod ui;
pub mod user_identity;

// Re-export commonly used types at crate root for convenience.
pub use error::AppError;
