# AI / LLM Provider

**Status:** Planned — not yet implemented.

---

## Overview

The LLM provider client handles all communication with language model APIs. It is a shared, immutable client initialized by the supervisor and passed as a capability handle to the Agents subsystem.

---

## Responsibilities

- Make requests to LLM provider APIs (OpenAI-compatible, Anthropic, local Ollama)
- Handle authentication (API key, OAuth token rotation)
- Model failover: primary → fallback provider on error
- Track token usage and cost per request
- Support both batch and streaming responses

---

## Planned Provider Support

| Provider | Auth | Notes |
|----------|------|-------|
| Anthropic | API key | Primary recommended provider |
| OpenAI | API key | Compatible fallback |
| OpenAI-compatible | API key | Azure, local Ollama, etc. |

---

## Configuration (planned)

```toml
[llm.primary]
provider = "anthropic"
model = "claude-opus-4.6"
api_url = "https://api.anthropic.com/v1"

[llm.fallback]
provider = "openai"
model = "gpt-4-turbo"
```

Secrets via env:
```bash
LLM_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
```
