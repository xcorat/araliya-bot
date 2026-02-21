//! [`SessionHandle`] — async-safe handle for reading and writing session data.
//!
//! `SessionHandle` is intentionally lightweight: it carries identity metadata
//! (`session_id`) and delegates all data read/write behavior to [`SessionRw`].

use std::path::PathBuf;
use std::sync::Arc;

use crate::error::AppError;
use crate::llm::{LlmUsage, ModelRates};

use super::collections::{Block, Doc};
pub use super::rw::SessionFileInfo;
use super::rw::SessionRw;
use super::store::{SessionStore, TranscriptEntry};
use super::stores::tmp::TmpStore;
use super::SessionSpend;

#[derive(Clone)]
pub struct SessionHandle {
    pub session_id: String,
    rw: Arc<SessionRw>,
}

impl SessionHandle {
    pub(crate) fn new(
        session_id: String,
        session_dir: PathBuf,
        stores: Vec<Arc<dyn SessionStore>>,
        tmp_store: Option<Arc<TmpStore>>,
    ) -> Self {
        Self {
            session_id,
            rw: Arc::new(SessionRw::new(session_dir, stores, tmp_store)),
        }
    }

    pub async fn kv_get(&self, key: &str) -> Result<Option<String>, AppError> {
        self.rw.kv_get(key).await
    }

    pub async fn kv_set(&self, key: &str, value: &str) -> Result<(), AppError> {
        self.rw.kv_set(key, value).await
    }

    pub async fn kv_delete(&self, key: &str) -> Result<bool, AppError> {
        self.rw.kv_delete(key).await
    }

    pub async fn transcript_append(&self, role: &str, content: &str) -> Result<(), AppError> {
        self.rw.transcript_append(role, content).await
    }

    pub async fn transcript_read_last(&self, n: usize) -> Result<Vec<TranscriptEntry>, AppError> {
        self.rw.transcript_read_last(n).await
    }

    pub async fn working_memory_read(&self) -> Result<String, AppError> {
        Ok(self.kv_get("working_memory").await?.unwrap_or_default())
    }

    pub async fn kv_doc(&self) -> Result<Doc, AppError> {
        self.rw.kv_doc().await
    }

    pub async fn transcript_block(&self) -> Result<Block, AppError> {
        self.rw.transcript_block().await
    }

    pub fn tmp_doc(&self) -> Result<Doc, AppError> {
        self.rw.tmp_doc()
    }

    pub fn tmp_block(&self) -> Result<Block, AppError> {
        self.rw.tmp_block()
    }

    pub fn set_tmp_doc(&self, doc: Doc) -> Result<(), AppError> {
        self.rw.set_tmp_doc(doc)
    }

    pub fn set_tmp_block(&self, block: Block) -> Result<(), AppError> {
        self.rw.set_tmp_block(block)
    }

    pub async fn list_files(&self) -> Result<Vec<SessionFileInfo>, AppError> {
        self.rw.list_files().await
    }

    /// Accumulate token usage from one LLM turn into `spend.json` for this session.
    ///
    /// Reads the current sidecar (or defaults to zero), adds the new counts,
    /// recomputes the incremental cost using `rates`, and writes the file back.
    /// Returns the updated [`SessionSpend`].
    pub async fn accumulate_spend(
        &self,
        usage: &LlmUsage,
        rates: &ModelRates,
    ) -> Result<SessionSpend, AppError> {
        let session_dir = self.rw.session_dir().to_path_buf();
        let usage = usage.clone();
        let rates = rates.clone();
        tokio::task::spawn_blocking(move || {
            accumulate_spend_blocking(&session_dir, &usage, &rates)
        })
        .await
        .map_err(|e| AppError::Memory(format!("spend spawn_blocking: {e}")))?
    }
}

// ── Spend helpers ─────────────────────────────────────────────────────────────

fn accumulate_spend_blocking(
    session_dir: &std::path::Path,
    usage: &LlmUsage,
    rates: &ModelRates,
) -> Result<SessionSpend, AppError> {
    let spend_path = session_dir.join("spend.json");

    let mut spend: SessionSpend = if spend_path.exists() {
        let raw = std::fs::read_to_string(&spend_path)
            .map_err(|e| AppError::Memory(format!("read spend.json: {e}")))?;
        serde_json::from_str(&raw)
            .map_err(|e| AppError::Memory(format!("parse spend.json: {e}")))?
    } else {
        SessionSpend::default()
    };

    spend.total_input_tokens += usage.input_tokens;
    spend.total_output_tokens += usage.output_tokens;
    spend.total_cached_tokens += usage.cached_input_tokens;
    spend.total_cost_usd += usage.cost_usd(rates);
    spend.last_updated = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let (y, mo, d, h, mi, s) = epoch_to_ymd_hms(secs);
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    };

    let data = serde_json::to_string_pretty(&spend)
        .map_err(|e| AppError::Memory(format!("serialize spend.json: {e}")))?;
    std::fs::write(&spend_path, &data)
        .map_err(|e| AppError::Memory(format!("write spend.json: {e}")))?;

    Ok(spend)
}

/// Convert Unix epoch seconds to (year, month, day, hour, min, sec).
/// Minimal implementation that avoids a `chrono` dependency.
fn epoch_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;

    let mut year = 1970u32;
    let mut remaining = days;
    loop {
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = [31u64, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0u32;
    for &dim in &days_in_month {
        if remaining < dim {
            break;
        }
        remaining -= dim;
        month += 1;
    }
    (year, month + 1, (remaining + 1) as u32, h as u32, m as u32, s as u32)
}

impl std::fmt::Debug for SessionHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionHandle")
            .field("session_id", &self.session_id)
            .field("session_dir", &self.rw.session_dir())
            .field("stores", &self.rw.store_types())
            .field("has_tmp_store", &self.rw.has_tmp_store())
            .finish()
    }
}
