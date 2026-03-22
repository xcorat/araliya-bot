//! Memory subsystem bus handler — exposes `memory/` methods on the supervisor bus.
//!
//! Part of the management plane: provides read-only queries over memory
//! subsystem data (e.g. knowledge-graph inspection) without owning any
//! mutable state.
//!
//! Currently exposes:
//! - `memory/kg_graph` — knowledge graph JSON for an agent's kgdocstore.
//! - `memory/status`   — subsystem status (required convention).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::oneshot;

use araliya_core::bus::ComponentStatusResponse;
use araliya_core::bus::{BusError, BusHandler, BusPayload, BusResult, ERR_METHOD_NOT_FOUND};

/// Bus handler for the `memory/` prefix.
pub struct MemoryBusHandler {
    /// agent_id → identity_dir, used to locate each agent's kgdocstore.
    agent_identity_dirs: Arc<HashMap<String, PathBuf>>,
}

impl MemoryBusHandler {
    pub fn new(agent_identity_dirs: Arc<HashMap<String, PathBuf>>) -> Self {
        Self {
            agent_identity_dirs,
        }
    }
}

impl BusHandler for MemoryBusHandler {
    fn prefix(&self) -> &str {
        "memory"
    }

    fn handle_request(
        &self,
        method: &str,
        payload: BusPayload,
        reply_tx: oneshot::Sender<BusResult>,
    ) {
        match method {
            "memory/kg_graph" => {
                let agent_id = match payload {
                    BusPayload::SessionQuery {
                        agent_id: Some(id), ..
                    } => id,
                    _ => {
                        let _ = reply_tx
                            .send(Err(BusError::new(-32600, "expected agent_id in payload")));
                        return;
                    }
                };

                let identity_dir = match self.agent_identity_dirs.get(&agent_id) {
                    Some(dir) => dir.clone(),
                    None => {
                        let _ = reply_tx.send(Err(BusError::new(
                            -32000,
                            format!("agent not found: {agent_id}"),
                        )));
                        return;
                    }
                };

                let kg_path = identity_dir
                    .join("kgdocstore")
                    .join("kg")
                    .join("graph.json");

                let body = match std::fs::read_to_string(&kg_path) {
                    Ok(json) => {
                        let graph = serde_json::from_str::<serde_json::Value>(&json)
                            .unwrap_or_else(
                                |_| serde_json::json!({"entities": {}, "relations": []}),
                            );
                        serde_json::json!({ "agent_id": agent_id, "graph": graph })
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        serde_json::json!({
                            "agent_id": agent_id,
                            "graph": { "entities": {}, "relations": [] }
                        })
                    }
                    Err(e) => {
                        let _ = reply_tx.send(Err(BusError::new(
                            -32000,
                            format!("failed to read KG graph: {e}"),
                        )));
                        return;
                    }
                };

                let _ = reply_tx.send(Ok(BusPayload::JsonResponse {
                    data: body.to_string(),
                }));
            }

            "memory/status" => {
                let data = ComponentStatusResponse::running("memory").to_json();
                let _ = reply_tx.send(Ok(BusPayload::JsonResponse { data }));
            }

            _ => {
                let _ = reply_tx.send(Err(BusError::new(
                    ERR_METHOD_NOT_FOUND,
                    format!("memory method not found: {method}"),
                )));
            }
        }
    }
}
