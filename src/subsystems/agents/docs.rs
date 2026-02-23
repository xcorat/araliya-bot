use std::fs;
use std::sync::Arc;

use tokio::sync::oneshot;

use crate::supervisor::bus::{BusError, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};
use super::{Agent, AgentsState};

pub(crate) struct DocsAgentPlugin;

impl Agent for DocsAgentPlugin {
    fn id(&self) -> &str { "docs" }

    fn handle(
        &self,
        action: String,
        channel_id: String,
        content: String,
        session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        tokio::spawn(async move {
            if action == "health" {
                let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                    channel_id,
                    content: "docs component: active".to_string(),
                    session_id,
                    usage: None,
                }));
                return;
            }

            // For now we only support a single "ask" action (alias for default).
            let query = if action == "ask" || action == "" || action == "handle" {
                content
            } else {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("unknown docs action: {action}"),
                )));
                return;
            };

            // Read markdown file (blocking I/O in spawn_blocking to avoid
            // holding tokio threads).
            let path_opt = state.docs_path.clone();
            let path = path_opt.unwrap_or_else(|| "docs/quick-intro.md".to_string());
            let file_content = tokio::task::spawn_blocking(move || fs::read_to_string(&path))
                .await
                .unwrap_or_else(|e| Err(e.into()))
                .map_err(|e| BusError::new(-32000, format!("failed to read docs file: {e}")));

            let file_content = match file_content {
                Ok(text) => text,
                Err(e) => {
                    let _ = reply_tx.send(Err(e));
                    return;
                }
            };

            let prompt = format!(
                "Answer using the docs. Limit chars < 4000 hard.\n\n### Document\n{}\n\n### Question\n{}\n\n### Answer:\n",
                file_content, query
            );

            let llm_result = state.complete_via_llm(&channel_id, &prompt).await;
            match llm_result {
                Ok(BusPayload::CommsMessage { content, usage, .. }) => {
                    let _ = reply_tx.send(Ok(BusPayload::CommsMessage {
                        channel_id,
                        content,
                        session_id,
                        usage,
                    }));
                }
                Ok(other) => {
                    let _ = reply_tx.send(Err(BusError::new(
                        -32000,
                        format!("unexpected LLM reply: {other:?}"),
                    )));
                }
                Err(e) => {
                    let _ = reply_tx.send(Err(e));
                }
            }
        });
    }
}
