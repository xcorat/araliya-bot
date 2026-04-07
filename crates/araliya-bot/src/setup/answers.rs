//! Collected wizard answers — populated step by step, then passed to the writer.

use std::path::PathBuf;

#[derive(Debug)]
#[allow(dead_code)] // all fields used by writer/doctor; llm_provider + config_dir for future expansion
pub struct Answers {
    // ── Identity ──────────────────────────────────────────────────────
    pub bot_name: String,
    /// Runtime data dir (identity keypair, sessions, memory) → ~/.araliya
    pub work_dir: PathBuf,
    /// App config dir (config.toml, .env) → ~/.config/araliya
    pub config_dir: PathBuf,

    // ── LLM ───────────────────────────────────────────────────────────
    pub llm_provider: LlmProvider,
    pub openai_api_key: String,
    pub llm_model: String,
    pub llm_api_base_url: String,

    // ── Agent profile ─────────────────────────────────────────────────
    pub profile: BotProfile,

    // ── Channels ──────────────────────────────────────────────────────
    pub enable_http: bool,
    pub http_bind: String,
    pub enable_telegram: bool,
    pub telegram_token: Option<String>,

    // ── Profile-specific extras ───────────────────────────────────────
    /// Homebuilder: display name shown on the generated landing page.
    pub homebuilder_user_name: Option<String>,
    /// Homebuilder: path to markdown notes directory.
    pub homebuilder_notes_dir: Option<String>,
    /// Docs / DocsKg: path to local docs directory.
    pub docs_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LlmProvider {
    OpenAI,
    OpenRouter,
    Anthropic,
    LocalOllama,
    OtherOpenAICompat,
    Dummy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BotProfile {
    BasicChat,
    SessionChat,
    AgenticChat,
    Docs,
    DocsKg,
    Homebuilder,
    Newsroom,
    Custom,
}
