use std::sync::Arc;
use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::api::{ApiClient, SessionInfo, SessionTranscriptMessage, HealthResponse, UsageInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivitySection {
    Chat,
    Memory,
    Tools,
    Status,
    Settings,
    Docs,
}

impl ActivitySection {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Memory => "Memory",
            Self::Tools => "Tools",
            Self::Status => "Status",
            Self::Settings => "Settings",
            Self::Docs => "Docs",
        }
    }
}

#[derive(Debug, Clone)]
pub enum LayoutMode {
    Desktop,
    Tablet,
    Compact,
}

impl LayoutMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Desktop => "Desktop",
            Self::Tablet => "Tablet",
            Self::Compact => "Compact",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayoutState {
    pub mode: LayoutMode,
    pub left_panel_open: bool,
    pub right_panel_open: bool,
    pub left_panel_width: f32,
    pub right_panel_width: f32,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            mode: LayoutMode::Desktop,
            left_panel_open: true,
            right_panel_open: false,
            left_panel_width: 260.0,
            right_panel_width: 320.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutPrefs {
    pub left_panel_open: bool,
    pub right_panel_open: bool,
    pub left_panel_width: f32,
    pub right_panel_width: f32,
    pub updated_at: String,
}

impl Default for LayoutPrefs {
    fn default() -> Self {
        Self {
            left_panel_open: true,
            right_panel_open: false,
            left_panel_width: 260.0,
            right_panel_width: 320.0,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

impl LayoutPrefs {
    pub fn into_layout(self) -> LayoutState {
        LayoutState {
            mode: LayoutMode::Desktop,
            left_panel_open: self.left_panel_open,
            right_panel_open: self.right_panel_open,
            left_panel_width: self.left_panel_width.clamp(220.0, 360.0),
            right_panel_width: self.right_panel_width.clamp(260.0, 420.0),
        }
    }

    pub fn from_layout(layout: &LayoutState) -> Self {
        Self {
            left_panel_open: layout.left_panel_open,
            right_panel_open: layout.right_panel_open,
            left_panel_width: layout.left_panel_width,
            right_panel_width: layout.right_panel_width,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

pub struct AppState {
    pub api_client: Arc<ApiClient>,
    pub active_section: ActivitySection,
    pub layout: LayoutState,
    pub health_status: Option<HealthResponse>,
    pub sessions: Vec<SessionInfo>,
    pub active_session_id: Option<String>,
    pub messages: Vec<SessionTranscriptMessage>,
    pub session_usage_totals: Option<UsageInfo>,
    pub is_loading_sessions: bool,
    pub is_loading_messages: bool,
    pub is_sending_message: bool,
    pub input_text: String,
}

impl AppState {
    pub fn new(api_client: Arc<ApiClient>) -> Self {
        Self {
            api_client,
            active_section: ActivitySection::Chat,
            layout: LayoutState::default(),
            health_status: None,
            sessions: Vec::new(),
            active_session_id: None,
            messages: Vec::new(),
            session_usage_totals: None,
            is_loading_sessions: false,
            is_loading_messages: false,
            is_sending_message: false,
            input_text: String::new(),
        }
    }

    pub fn with_layout(api_client: Arc<ApiClient>, layout: LayoutState) -> Self {
        let mut state = Self::new(api_client);
        state.layout = layout;
        state
    }
}

pub const DESKTOP_BREAKPOINT_PX: f32 = 1200.0;
pub const TABLET_BREAKPOINT_PX: f32 = 860.0;

fn layout_prefs_path() -> Option<PathBuf> {
    let mut config_dir = dirs::config_dir()?;
    config_dir.push("araliya-bot");
    config_dir.push("gpui-layout.json");
    Some(config_dir)
}

pub fn load_layout_prefs() -> Option<LayoutPrefs> {
    let path = layout_prefs_path()?;
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<LayoutPrefs>(&raw).ok()
}

pub fn save_layout_prefs(layout: &LayoutState) {
    let Some(path) = layout_prefs_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    let prefs = LayoutPrefs::from_layout(layout);
    let Ok(raw) = serde_json::to_string_pretty(&prefs) else {
        return;
    };

    let _ = fs::write(path, raw);
}
