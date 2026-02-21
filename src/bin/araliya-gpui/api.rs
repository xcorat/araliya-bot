use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStep {
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    pub session_id: String,
    pub mode: String,
    pub run_id: Option<String>,
    pub reply: String,
    pub working_memory_updated: bool,
    pub intermediate_steps: Option<Vec<ToolStep>>,
    pub usage: Option<UsageInfo>,
    pub session_usage_totals: Option<UsageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub bot_id: String,
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_timeout_seconds: u32,
    pub enabled_tools: Vec<String>,
    pub max_tool_rounds: u32,
    pub session_count: u32,
    pub uptime_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTranscriptMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Vec<SessionToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolCall {
    pub id: String,
    pub r#type: Option<String>,
    pub function: SessionToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetailResponse {
    pub session_id: String,
    pub transcript: Vec<SessionTranscriptMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRequest {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

pub struct ApiClient {
    base_url: String,
    client: reqwest::Client,
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    pub async fn check_health(&self) -> Result<HealthResponse, reqwest::Error> {
        self.client
            .get(&format!("{}/api/health", self.base_url))
            .send()
            .await?
            .json()
            .await
    }

    pub async fn list_sessions(&self) -> Result<SessionsResponse, reqwest::Error> {
        self.client
            .get(&format!("{}/api/sessions", self.base_url))
            .send()
            .await?
            .json()
            .await
    }

    pub async fn get_session_by_id(&self, session_id: &str) -> Result<SessionDetailResponse, reqwest::Error> {
        self.client
            .get(&format!("{}/api/session/{}", self.base_url, session_id))
            .send()
            .await?
            .json()
            .await
    }

    pub async fn send_message(&self, message: String, session_id: Option<String>) -> Result<MessageResponse, reqwest::Error> {
        let req = MessageRequest {
            message,
            session_id,
            mode: None,
        };
        self.client
            .post(&format!("{}/api/message", self.base_url))
            .json(&req)
            .send()
            .await?
            .json()
            .await
    }
}
