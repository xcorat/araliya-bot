use std::sync::Arc;
use crate::api::{ApiClient, SessionInfo, SessionTranscriptMessage, HealthResponse};

pub struct AppState {
    pub api_client: Arc<ApiClient>,
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
