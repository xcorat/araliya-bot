use std::sync::Arc;
use crate::api::{ApiClient, SessionInfo, SessionTranscriptMessage, HealthResponse};

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
pub struct LayoutState {
    pub left_panel_open: bool,
    pub right_panel_open: bool,
}

pub struct AppState {
    pub api_client: Arc<ApiClient>,
    pub active_section: ActivitySection,
    pub layout: LayoutState,
    pub health_status: Option<HealthResponse>,
    pub sessions: Vec<SessionInfo>,
    pub active_session_id: Option<String>,
    pub messages: Vec<SessionTranscriptMessage>,
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
            layout: LayoutState {
                left_panel_open: true,
                right_panel_open: false,
            },
            health_status: None,
            sessions: Vec::new(),
            active_session_id: None,
            messages: Vec::new(),
            is_loading_sessions: false,
            is_loading_messages: false,
            is_sending_message: false,
            input_text: String::new(),
        }
    }
}
