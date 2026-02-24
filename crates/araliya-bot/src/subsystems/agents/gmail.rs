use std::sync::Arc;

use tokio::sync::oneshot;

use crate::supervisor::bus::{BusPayload, BusResult};

use super::{Agent, AgentsState};

pub(crate) struct GmailAgentPlugin;

impl Agent for GmailAgentPlugin {
    fn id(&self) -> &str { "gmail" }

    fn handle(
        &self,
        action: String,
        channel_id: String,
        _content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        tokio::spawn(async move {
            if action != "read" {
                let _ = reply_tx.send(Err(crate::supervisor::bus::BusError::new(
                    crate::supervisor::bus::ERR_METHOD_NOT_FOUND,
                    format!("unknown gmail action: {action}"),
                )));
                return;
            }

            let result = state
                .execute_tool(
                    "gmail",
                    "read_latest",
                    serde_json::json!({}).to_string(),
                    &channel_id,
                    session_id.clone(),
                )
                .await;

            match result {
                Ok(BusPayload::ToolResponse { ok: true, data_json: Some(data), .. }) => {
                    let summary = serde_json::from_str::<serde_json::Value>(&data)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    let content = format!(
                        "From: {}\nSubject: {}\nDate: {}\nSnippet: {}",
                        summary.get("from").and_then(|v| v.as_str()).unwrap_or(""),
                        summary.get("subject").and_then(|v| v.as_str()).unwrap_or(""),
                        summary.get("date").and_then(|v| v.as_str()).unwrap_or(""),
                        summary.get("snippet").and_then(|v| v.as_str()).unwrap_or(""),
                    );
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content,
                        session_id,
                        usage: None,
                    }));
                }
                Ok(BusPayload::ToolResponse { ok: false, error, .. }) => {
                    let _ = reply_tx.send(Err(crate::supervisor::bus::BusError::new(
                        -32000,
                        format!("gmail tool error: {}", error.unwrap_or_else(|| "unknown".to_string())),
                    )));
                }
                Ok(other) => {
                    let _ = reply_tx.send(Err(crate::supervisor::bus::BusError::new(
                        -32000,
                        format!("unexpected tools reply: {other:?}"),
                    )));
                }
                Err(e) => {
                    let _ = reply_tx.send(Err(e));
                }
            }
        });
    }
}
