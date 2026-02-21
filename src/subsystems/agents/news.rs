use std::sync::Arc;

use tokio::sync::oneshot;

use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};

use super::{Agent, AgentsState};

pub(crate) struct NewsAgentPlugin;

impl Agent for NewsAgentPlugin {
    fn id(&self) -> &str { "news" }

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
            if action == "health" {
                let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                    channel_id,
                    content: "news component: active".to_string(),
                    session_id,
                    usage: None,
                }));
                return;
            }

            let tool_action = match action.as_str() {
                "handle" | "read" => "get",
                _ => {
                    let _ = reply_tx.send(Err(BusError::new(
                        ERR_METHOD_NOT_FOUND,
                        format!("unknown news action: {action}"),
                    )));
                    return;
                }
            };

            let result = state
                .execute_tool(
                    "newsmail_aggregator",
                    tool_action,
                    state.news_query_args_json.clone(),
                    &channel_id,
                    session_id.clone(),
                )
                .await;

            match result {
                Ok(BusPayload::ToolResponse { ok: true, data_json: Some(data), .. }) => {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: data,
                        session_id,
                        usage: None,
                    }));
                }
                Ok(BusPayload::ToolResponse { ok: true, data_json: None, .. }) => {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content: "[]".to_string(),
                        session_id,
                        usage: None,
                    }));
                }
                Ok(BusPayload::ToolResponse { ok: false, error, .. }) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32000,
                        format!(
                            "newsmail_aggregator tool error: {}",
                            error.unwrap_or_else(|| "unknown".to_string())
                        ),
                    )));
                }
                Ok(other) => {
                    let _ = reply_tx.send(Err(BusError::new(
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
