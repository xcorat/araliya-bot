use std::path::PathBuf;
use tracing::{debug, error, info, warn};

use araliya_core::config::{ApiType, ProviderConfig, resolve_api_key, Config};
use rusqlite::Connection;

/// Initializes the dynamic SQLite configuration database and loads any
/// user-defined LLM providers into the runtime config.
///
/// This database lives in the user's `data_local_dir` (e.g. `~/.local/share/araliya/araliya.db`)
/// and overrides/augments the base TOML configuration.
pub fn load_providers(config: &mut Config) {
    let data_dir = dirs::data_local_dir().unwrap_or_else(|| {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join(".local/share")
    });
    
    let araliya_dir = data_dir.join("araliya");
    if let Err(e) = std::fs::create_dir_all(&araliya_dir) {
        error!(error = %e, path = %araliya_dir.display(), "failed to create araliya appdata directory");
        return;
    }

    let db_path = araliya_dir.join("araliya.db");
    let conn = match Connection::open(&db_path) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, path = %db_path.display(), "failed to open SQLite database");
            return;
        }
    };

    // Initialize the llm_providers table if it doesn't exist.
    let create_sql = "
    CREATE TABLE IF NOT EXISTS llm_providers (
        name TEXT PRIMARY KEY,
        api_type TEXT NOT NULL,
        api_base_url TEXT NOT NULL,
        model TEXT NOT NULL,
        temperature REAL NOT NULL,
        api_key TEXT,
        api_key_file TEXT,
        reasoning_effort TEXT,
        timeout_seconds INTEGER NOT NULL,
        max_tokens INTEGER NOT NULL,
        input_per_million_usd REAL NOT NULL,
        output_per_million_usd REAL NOT NULL,
        cached_input_per_million_usd REAL NOT NULL
    );";

    if let Err(e) = conn.execute(create_sql, []) {
        error!(error = %e, "failed to initialize llm_providers table");
        return;
    }

    // Load available providers.
    let mut stmt = match conn.prepare("SELECT * FROM llm_providers") {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "failed to prepare SELECT on llm_providers");
            return;
        }
    };

    let provider_iter = match stmt.query_map([], |row| {
        let name: String = row.get(0)?;
        let api_type_str: String = row.get(1)?;
        let api_base_url: String = row.get(2)?;
        let model: String = row.get(3)?;
        let temperature: f64 = row.get(4)?;
        let api_key: Option<String> = row.get(5)?;
        let api_key_file: Option<String> = row.get(6)?;
        let reasoning_effort: Option<String> = row.get(7)?;
        let timeout_seconds: i64 = row.get(8)?;
        let max_tokens: i64 = row.get(9)?;
        let input_cost: f64 = row.get(10)?;
        let output_cost: f64 = row.get(11)?;
        let cached_input_cost: f64 = row.get(12)?;

        let api_type = match api_type_str.as_str() {
            "chat_completions" | "openai_compatible" => ApiType::ChatCompletions,
            "openai_responses" | "responses" => ApiType::OpenAiResponses,
            "dummy" => ApiType::Dummy,
            _ => ApiType::ChatCompletions,
        };

        let resolved_api_key = resolve_api_key(api_key, api_key_file);

        Ok((name, ProviderConfig {
            api_type,
            api_base_url,
            model,
            temperature: temperature as f32,
            api_key: resolved_api_key,
            reasoning_effort,
            timeout_seconds: timeout_seconds as u64,
            max_tokens: max_tokens as usize,
            input_per_million_usd: input_cost,
            output_per_million_usd: output_cost,
            cached_input_per_million_usd: cached_input_cost,
        }))
    }) {
        Ok(it) => it,
        Err(e) => {
            error!(error = %e, "failed to execute query on llm_providers");
            return;
        }
    };

    let mut loaded_count = 0;
    for provider_res in provider_iter {
        match provider_res {
            Ok((name, details)) => {
                debug!(name = %name, "loaded LLM provider from SQLite");
                config.llm.providers.insert(name, details);
                loaded_count += 1;
            }
            Err(e) => {
                warn!(error = %e, "failed to parse a row from llm_providers");
            }
        }
    }

    if loaded_count > 0 {
        info!(count = loaded_count, path = %db_path.display(), "merged dynamic LLM providers from database");
    }
}
