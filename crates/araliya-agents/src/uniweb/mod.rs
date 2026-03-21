//! `uniweb` agent plugin — shared-session "front-porch" agentic chat.
//!
//! All visitors to `/ui/chat` see the same global conversation.  Requests are
//! serialised through a [`tokio::sync::Semaphore`] so only one LLM call runs
//! at a time; concurrent callers block until the active turn completes.
//!
//! When `docsdir` is configured, the agent gains a `docs_search` local tool
//! and runs the full agentic instruction→tool→response loop (like
//! `agentic-chat`).  Without `docsdir` it falls back to plain session chat.
//!
//! The global session ID is deterministic: derived from the `"uniweb"` agent
//! seed, or explicitly set via `[agents.uniweb] session_id` in config.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::{Semaphore, oneshot};
use tracing::info;

use super::core::agentic::{AgenticLoop, LocalTool};
use super::{Agent, AgentsState};
use araliya_core::error::AppError;
use araliya_core::bus::message::{BusError, BusResult};

#[cfg(feature = "idocstore")]
use super::docs::DocsRagTool;
#[cfg(feature = "idocstore")]
use araliya_memory::stores::docstore::IDocStore;

// ── UniwebAgent ───────────────────────────────────────────────────────────────

pub(crate) struct UniwebAgent {
    /// The single global session ID used by all visitors.
    global_session_id: String,
    /// Permit gate — only one LLM call at a time.
    semaphore: Arc<Semaphore>,
    /// Number of requests currently waiting for the semaphore.
    pub(crate) queue_depth: Arc<AtomicUsize>,
    /// Whether to route the instruction pass through `llm/instruct`.
    use_instruction_llm: bool,
}

impl UniwebAgent {
    /// Create a new `UniwebAgent`.
    ///
    /// If `configured_session_id` is non-empty it is used as the global
    /// session ID; otherwise a deterministic ID is derived from `"uniweb"`.
    pub fn new(configured_session_id: &str, use_instruction_llm: bool) -> Self {
        let global_session_id = if configured_session_id.is_empty() {
            derive_session_id("uniweb")
        } else {
            configured_session_id.to_string()
        };

        info!(
            session_id = %global_session_id,
            use_instruction_llm,
            "uniweb: initialised"
        );

        Self {
            global_session_id,
            semaphore: Arc::new(Semaphore::new(1)),
            queue_depth: Arc::new(AtomicUsize::new(0)),
            use_instruction_llm,
        }
    }

    /// Build the [`AgenticLoop`] for the current request, including the
    /// `docs_search` local tool when a populated docstore is available.
    fn build_loop(&self, state: &AgentsState) -> AgenticLoop {
        let allowed_tools = state
            .agent_skills
            .get("uniweb")
            .cloned()
            .unwrap_or_default();

        let memory_tools = Self::build_memory_tools(state);

        AgenticLoop::new(
            "uniweb",
            self.use_instruction_llm,
            "instruct.md",
            "context.md",
            vec![],
            memory_tools,
            allowed_tools,
            &state.agents_dir,
            state.debug_logging,
        )
    }

    /// Construct the `docs_search` local tool if the docstore is populated.
    fn build_memory_tools(state: &AgentsState) -> Vec<Arc<dyn LocalTool + Send + Sync>> {
        #[cfg(feature = "idocstore")]
        {
            if let Some(docs_cfg) = state.agent_docs.get("uniweb") {
                if let Some(identity) = state.agent_identities.get("uniweb") {
                    let dir = &identity.identity_dir;
                    let populated = IDocStore::open(dir)
                        .and_then(|s| s.list_documents())
                        .map(|docs| !docs.is_empty())
                        .unwrap_or(false);
                    if populated {
                        let tool: Arc<dyn LocalTool + Send + Sync> = Arc::new(DocsRagTool {
                            identity_dir: dir.clone(),
                            index_name: docs_cfg
                                .index
                                .clone()
                                .unwrap_or_else(|| "index.md".to_string()),
                            use_kg: docs_cfg.use_kg,
                            kg_cfg: docs_cfg.kg.clone(),
                        });
                        return vec![tool];
                    }
                }
            }
        }
        #[cfg(not(feature = "idocstore"))]
        let _ = state;
        vec![]
    }

    async fn ensure_global_session(
        state: Arc<AgentsState>,
        global_session_id: String,
    ) -> Result<(), AppError> {
        tokio::task::spawn_blocking(move || {
            let memory = state.memory.clone();
            let store_types = state
                .agent_memory
                .get("uniweb")
                .cloned()
                .unwrap_or_else(|| vec!["basic_session".to_string()]);
            let store_type_refs: Vec<&str> = store_types.iter().map(String::as_str).collect();
            let store = state.open_agent_store("uniweb")?;
            store.get_or_create_session_with_id(
                &memory,
                "uniweb",
                &global_session_id,
                &store_type_refs,
            )?;
            Ok(())
        })
        .await
        .map_err(|e| AppError::Memory(format!("uniweb session bootstrap task failed: {e}")))?
    }
}

impl Agent for UniwebAgent {
    fn id(&self) -> &str {
        "uniweb"
    }

    fn handle(
        &self,
        _action: String,
        channel_id: String,
        content: String,
        _session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        let sem = self.semaphore.clone();
        let depth = self.queue_depth.clone();
        let gsid = self.global_session_id.clone();
        let loop_ = self.build_loop(&state);
        let bootstrap_state = state.clone();

        tokio::spawn(async move {
            depth.fetch_add(1, Ordering::Relaxed);
            let _permit = sem.acquire().await.expect("semaphore closed");
            depth.fetch_sub(1, Ordering::Relaxed);

            if let Err(e) = Self::ensure_global_session(bootstrap_state, gsid.clone()).await {
                let _ = reply_tx.send(Err(BusError::new(
                    -32000,
                    format!("uniweb session bootstrap failed: {e}"),
                )));
                return;
            }

            // Run the full agentic loop, pinning to the global session.
            let result = loop_.run(channel_id, content, Some(gsid), state).await;
            let _ = reply_tx.send(result);
        });
    }

    fn handle_stream(
        &self,
        channel_id: String,
        content: String,
        _session_id: Option<String>,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    ) {
        let sem = self.semaphore.clone();
        let depth = self.queue_depth.clone();
        let gsid = self.global_session_id.clone();
        let loop_ = self.build_loop(&state);
        let bootstrap_state = state.clone();

        tokio::spawn(async move {
            depth.fetch_add(1, Ordering::Relaxed);
            let _permit = sem.acquire().await.expect("semaphore closed");
            depth.fetch_sub(1, Ordering::Relaxed);

            if let Err(e) = Self::ensure_global_session(bootstrap_state, gsid.clone()).await {
                let _ = reply_tx.send(Err(BusError::new(
                    -32000,
                    format!("uniweb session bootstrap failed: {e}"),
                )));
                return;
            }

            let result = loop_
                .run_stream(channel_id, content, Some(gsid), state)
                .await;
            let _ = reply_tx.send(result);
        });
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Derive a deterministic UUID v4-shaped session ID from the bot identity.
fn derive_session_id(bot_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"uniweb-global-session:");
    hasher.update(bot_id.as_bytes());
    let hash = hasher.finalize();
    // Format first 16 bytes as a UUID v4.
    let bytes: [u8; 16] = hash[..16].try_into().unwrap();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        u32::from_be_bytes(bytes[0..4].try_into().unwrap()),
        u16::from_be_bytes(bytes[4..6].try_into().unwrap()),
        u16::from_be_bytes(bytes[6..8].try_into().unwrap()) & 0x0FFF,
        (u16::from_be_bytes(bytes[8..10].try_into().unwrap()) & 0x3FFF) | 0x8000,
        // Last 6 bytes as a 48-bit value.
        u64::from_be_bytes({
            let mut buf = [0u8; 8];
            buf[2..8].copy_from_slice(&bytes[10..16]);
            buf
        }),
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::derive_session_id;

    #[test]
    fn derived_session_id_is_stable() {
        let id1 = derive_session_id("testbot123");
        let id2 = derive_session_id("testbot123");
        assert_eq!(id1, id2, "same bot_id must produce the same session ID");
    }

    #[test]
    fn derived_session_id_looks_like_uuid() {
        let id = derive_session_id("testbot123");
        // UUID v4 format: 8-4-4-4-12
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert!(parts[2].starts_with('4'), "version nibble must be 4");
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
    }

    #[test]
    fn different_bot_ids_differ() {
        let id1 = derive_session_id("bot-a");
        let id2 = derive_session_id("bot-b");
        assert_ne!(id1, id2);
    }
}
