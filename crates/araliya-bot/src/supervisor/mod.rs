//! Supervisor — re-exported from `araliya-supervisor`.

pub mod adapters;
pub mod bus;
pub mod component_info;
pub mod control;
pub mod dispatch;
pub mod health;

pub use araliya_supervisor::run::run;
