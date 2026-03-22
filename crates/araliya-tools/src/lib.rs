//! Tools subsystem — external tool integrations (Gmail, GDELT BigQuery, RSS).

pub mod dispatcher;
#[cfg(feature = "plugin-gdelt-tool")]
pub mod gdelt_bigquery;
#[cfg(feature = "plugin-gmail-tool")]
pub mod gmail;
#[cfg(feature = "plugin-gmail-tool")]
pub mod newsmail_aggregator;
#[cfg(feature = "plugin-rss-fetch-tool")]
pub mod rss_fetch;

#[cfg(feature = "subsystem-tools")]
pub use dispatcher::ToolsSubsystem;
