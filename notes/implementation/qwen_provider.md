# Qwen LLM Provider (2026-02-22)

## Summary

Added a dedicated `qwen` LLM provider path in the LLM subsystem while reusing the existing OpenAI-compatible HTTP implementation internally.

## Changes

- Added `src/llm/providers/qwen.rs` with `QwenProvider`.
- Registered provider in `src/llm/providers/mod.rs` under selector `"qwen"`.
- Extended `LlmProvider` enum in `src/llm/mod.rs` with `LlmProvider::Qwen`.
- Added `[llm.qwen]` config support in `src/config.rs` with defaults:
  - `api_base_url = "http://127.0.0.1:8081/v1/chat/completions"`
  - `model = "qwen2.5-instruct"`
  - `temperature = 0.2`
  - `timeout_seconds = 60`
- Added `config/default.toml` section for `[llm.qwen]`.
- Updated cost-rate selection in `src/subsystems/llm/mod.rs` so `llm.default = "qwen"` uses qwen pricing fields.

## Usage

Set in TOML:

```toml
[llm]
default = "qwen"

[llm.qwen]
api_base_url = "http://127.0.0.1:8081/v1/chat/completions"
model = "qwen2.5-instruct"
temperature = 0.2
timeout_seconds = 60
```

`LLM_API_KEY` remains optional for local keyless endpoints.
