//! Component info — shared type for describing the runtime component tree.
//!
//! Every [`BusHandler`] subsystem can implement [`BusHandler::component_info`]
//! to expose its children. The supervisor uses this to build the full tree
//! for `ControlCommand::ComponentTree`. Non-bus subsystems (e.g. comms) pass
//! their info via an `Arc<OnceLock<ComponentInfo>>` registered at startup.

use serde::{Deserialize, Serialize};

// ── ComponentStatus ───────────────────────────────────────────────────────────

/// Runtime state of a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComponentStatus {
    /// Component is loaded and operating normally.
    On,
    /// Component is loaded but intentionally inactive.
    Off,
    /// Component has encountered an error.
    Err,
}

impl ComponentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ComponentStatus::On => "on",
            ComponentStatus::Off => "off",
            ComponentStatus::Err => "err",
        }
    }
}

// ── ComponentInfo ─────────────────────────────────────────────────────────────

/// Description of a single component node for the management tree.
///
/// Returned by [`crate::supervisor::dispatch::BusHandler::component_info`].
/// The tree is serialised to JSON for `manage/tree`, `manage/http/tree`,
/// and `ControlCommand::ComponentTree`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// Stable machine identifier (e.g. `"agents"`, `"echo"`, `"http0"`).
    pub id: String,
    /// Human-readable display name (e.g. `"Agents"`, `"Echo"`, `"HTTP"`).
    pub name: String,
    /// Lifecycle status string (`"running"` or `"stopped"`).
    pub status: String,
    /// Operational state.
    pub state: ComponentStatus,
    /// Optional uptime in milliseconds (supervisor root only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_ms: Option<u64>,
    /// Child components, sorted by id.
    pub children: Vec<ComponentInfo>,
}

// ── ComponentStatusResponse ───────────────────────────────────────────────────

/// Response payload for `{prefix}/status` and `{prefix}/{child_id}/status` bus routes.
///
/// Serialised as `BusPayload::JsonResponse { data }`.  Every component that
/// implements the status convention returns this shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatusResponse {
    /// Component identifier (matches the node id in the management tree).
    pub id: String,
    /// Current operational status string (e.g. `"running"`, `"stopped"`, `"error"`).
    pub status: String,
    /// Operational state flag.
    pub state: ComponentStatus,
}

impl ComponentStatusResponse {
    /// A running / healthy component.
    pub fn running(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: "running".to_string(),
            state: ComponentStatus::On,
        }
    }

    /// A stopped / inactive component.
    pub fn stopped(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: "stopped".to_string(),
            state: ComponentStatus::Off,
        }
    }

    /// A component in an error / degraded state.
    pub fn error(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: message.into(),
            state: ComponentStatus::Err,
        }
    }

    /// Serialise to a JSON string for use in `BusPayload::JsonResponse { data }`.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

// ── ComponentInfo ─────────────────────────────────────────────────────────────

impl ComponentInfo {
    /// A running node with children.
    pub fn running(id: &str, name: &str, children: Vec<ComponentInfo>) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            status: "running".to_string(),
            state: ComponentStatus::On,
            uptime_ms: None,
            children,
        }
    }

    /// A running leaf node (no children).
    pub fn leaf(id: &str, name: &str) -> Self {
        Self::running(id, name, vec![])
    }

    /// Sort children alphabetically by id (in-place).
    pub fn sort_children(&mut self) {
        self.children.sort_by(|a, b| a.id.cmp(&b.id));
    }

    /// Capitalise the first character of a string — convenience for turning
    /// an id like `"agents"` into a display name like `"Agents"`.
    pub fn capitalise(s: &str) -> String {
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        }
    }
}
