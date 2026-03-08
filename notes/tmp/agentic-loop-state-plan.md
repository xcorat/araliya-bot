# Instruction Loop: Testing & Debugging Scaffolding

## Context

The `AgenticLoop` in `core/agentic.rs` implements a 3-phase flow:
1. **Instruction pass** — small model (`llm/instruct`) returns JSON tool calls
2. **Tool execution** — local tools + bus dispatch, outputs collected as `context`
3. **Response pass** — large model (`llm/complete`) receives context + history + user input

The infrastructure is complete: `use_instruction_llm`, `[llm.instruction]` config inheritance, `debug_logging` to session KV. But:
- **Docs are out of sync** — `agents.md` is missing `agentic-chat`, `runtime_cmd`, `AgenticLoop`, `debug_logging`; `llm.md` is missing `llm/instruct` method
- **No integration tests** — only `parse_tool_calls` unit tests exist
- **No way to read debug data** — debug KV keys (`debug:turn:N:*`) are written but not exposed via API
- **Instruction prompt is too eager** — from `notes/tmp/agent-llm-message-flow.md`, the small model calls tools for greetings; the prompt can be tightened with negative examples

`config/docs_instr.toml` (untracked) is already a working dual-model overlay with `use_instruction_llm = true` and `[llm.instruction]`.

---

## Step 1 — Docs alignment (do this first)

### `docs/architecture/subsystems/agents.md`
- Add `agentic-chat` and `runtime_cmd` to the agents table
- Add **Agentic Loop** section after the agents table:
  - 3-phase flow diagram (instruction pass → tool execution → response pass)
  - `AgenticLoop` struct and constructor params
  - `LocalTool` vs bus tool distinction
  - `debug_logging = true` → `debug:turn:{n}:*` KV keys written per turn
  - `use_instruction_llm` flag and how it routes to `llm/instruct`

### `docs/architecture/subsystems/llm.md`
- Add `llm/instruct` bus method alongside `llm/complete`
- Note: falls back to main provider when `[llm.instruction]` is not configured

---

## Step 2 — Tighten instruction prompt

**File:** `config/prompts/agentic_instruct.txt`

Add an explicit examples section that teaches the model when NOT to call tools:

```
Examples of inputs that should return []:
- "hi" / "hello" / "thanks"
- "explain X" (factual, no external data needed)
- "what is Y?"
- General conversation and follow-up questions

Only call a tool when external, real-time, or personalized data is required.
```

---

## Step 3 — Integration tests for AgenticLoop

**File:** `crates/araliya-bot/src/subsystems/agents/mod.rs` (add to existing `#[cfg(test)]`)

The existing tests already construct a full `AgentsSubsystem` with a real in-memory bus and `TmpMemory`. The `DummyProvider` echoes input. With DummyProvider:
- Instruction pass returns `[echo] <prompt>` → `parse_tool_calls` returns `[]` (graceful degradation)
- Response pass returns `[echo] <context prompt>` → valid content

### Tests to add:

1. **`test_agentic_chat_returns_session_id`**
   - Enable `agentic-chat` in config (`use_instruction_llm: false`)
   - Send `CommsMessage` via `handle_request`
   - Assert reply is `Ok(BusPayload::CommsMessage { session_id: Some(_), .. })`

2. **`test_agentic_chat_debug_logging_writes_kv`**
   - Same setup but `debug_logging: true`
   - Send a message
   - Load the session handle from memory
   - Assert `debug:turn:1:user_input`, `debug:turn:1:instruct_prompt`, `debug:turn:1:instruction_response` all exist

3. **`test_agentic_chat_second_turn_reuses_session`**
   - Send first message, capture `session_id`
   - Send second message with that `session_id`
   - Assert reply includes the same `session_id` (session persists)

---

## Step 4 — Debug introspection API

### Bus method: `agents/{agent_id}/sessions/{session_id}/debug`

**File:** `crates/araliya-bot/src/subsystems/agents/mod.rs` (in `handle_request`)

- Parse `agent_id` and `session_id` from method path
- Load session from `agent_store.agent_sessions_dir()`
- Read all KV keys matching prefix `debug:turn:`
- Return `JsonResponse { data: json_string }` with structure:
  ```json
  {
    "session_id": "...",
    "turn_count": 3,
    "turns": {
      "1": { "user_input": "...", "instruct_prompt": "...", "instruction_response": "...", "tool_calls_json": "...", "context": "...", "response_prompt": "..." },
      ...
    }
  }
  ```

### HTTP endpoint: `GET /api/agents/{agent_id}/sessions/{session_id}/debug`

**Files:**
- `crates/araliya-bot/src/subsystems/comms/axum_channel.rs` — add route
- `crates/araliya-bot/src/subsystems/comms/http.rs` — add legacy route (if applicable)

Route dispatches `agents/{agent_id}/sessions/{session_id}/debug` on the bus and returns JSON.

---

## Step 5 — `config/docs_instr.toml` — add `debug_logging`

Add `debug_logging = true` under `[agents]` so the reference overlay enables debug introspection out of the box. This is the file to use when running the dual-model loop locally.

---

## Critical files

| File | Change |
|------|--------|
| `docs/architecture/subsystems/agents.md` | Add `agentic-chat`, `runtime_cmd`, AgenticLoop section |
| `docs/architecture/subsystems/llm.md` | Add `llm/instruct` bus method |
| `config/prompts/agentic_instruct.txt` | Add negative examples for when NOT to call tools |
| `crates/araliya-bot/src/subsystems/agents/mod.rs` | Add 3 integration tests + `debug` bus method handler |
| `crates/araliya-bot/src/subsystems/comms/axum_channel.rs` | Add `/api/agents/{id}/sessions/{sid}/debug` route |
| `config/docs_instr.toml` | Add `debug_logging = true` |

---

## Verification

1. `cargo test` — all existing tests pass + 3 new agentic loop tests pass
2. `cargo run -- -f config/docs_instr.toml` — starts with dual-model config
3. Send "hi" via PTY or UI, then:
   - `GET /api/agents/agentic-chat/sessions/{sid}/debug` → see instruction pass chose `[]`
4. Send a news-related query → see tool call in debug output
