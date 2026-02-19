# Plugin Interfaces

**Status:** v0.3.0 — agents (`src/subsystems/agents/mod.rs`), LLM (`src/llm/mod.rs`), BusHandler (`src/supervisor/dispatch.rs`)

This document specifies the three extension points in the architecture: how to add a new agent, a new LLM provider, and how subsystems register on the bus.

> **Naming convention:** `Agent` refers to autonomous actors in the agents subsystem.
> `Plugin` is reserved for capability extensions in the future tools subsystem.

---

## `Agent` — adding a new agent

```rust
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn handle(
        &self,
        channel_id: String,
        content: String,
        reply_tx: oneshot::Sender<BusResult>,
        state: Arc<AgentsState>,
    );
}
```

### Contract

- `id()` must match the name used in config routing and `[agents.routing]` values.
- `handle` must **not block the caller**:
  - Synchronous agents resolve `reply_tx` immediately (see `EchoAgent`).
  - Async agents `tokio::spawn` a task and resolve `reply_tx` from within it (see `BasicChatPlugin`, `SessionChatPlugin`).
- `reply_tx` is `oneshot::Sender<BusResult>` — consume it exactly once. Dropping it without sending causes the caller to receive `BusCallError::Recv`.
- `state: Arc<AgentsState>` is the capability surface. Do not circumvent it to access raw bus handles.

### `AgentsState` capability surface

```rust
impl AgentsState {
    pub async fn complete_via_llm(&self, channel_id: &str, content: &str) -> BusResult;
}

// Fields:
pub memory: Option<Arc<MemorySystem>>,  // when subsystem-memory is enabled
pub agent_memory: HashMap<String, Vec<String>>,  // per-agent store requirements
```

Agents call typed methods on `AgentsState` rather than addressing arbitrary bus targets. The raw `BusHandle` is private to the agents module.

### Built-in agents

| ID | Feature | Behaviour |
|----|---------|----------|
| `echo` | `plugin-echo` | Returns input unchanged; synchronous. |
| `basic_chat` | `plugin-basic-chat` | Delegates to `ChatCore::basic_complete` in a spawned task. |
| `chat` | `plugin-chat` | Session-aware chat via `SessionChatPlugin`; creates a memory session on first message, appends user/assistant transcript entries, injects recent history as LLM context. Requires `subsystem-memory`. |

### Adding an agent

1. Implement `Agent` in a new file under `src/subsystems/agents/`.
   - For chat-family agents, add to `src/subsystems/agents/chat/` and compose with `ChatCore`.
2. Add a Cargo feature gate (e.g. `plugin-myagent = ["subsystem-agents"]`).
3. Register it in `AgentsSubsystem::new()` behind `#[cfg(feature = "plugin-myagent")]`.
4. Add `[agents.myagent]` in `config/default.toml`.
5. If the agent needs memory, add `memory = ["basic_session"]` to its config section.

---

## `BusHandler` — registering a subsystem

```rust
pub trait BusHandler: Send + Sync {
    fn prefix(&self) -> &str;
    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>);
    fn handle_notification(&self, _method: &str, _payload: BusPayload) {}
}
```

See [bus-protocol.md](bus-protocol.md#bushandler-registration-contract) for the full specification. Key rules:

- `prefix()` is a string owned exclusively by this handler. The supervisor panics at startup on duplicates.
- `handle_request` receives the **full method string** including the prefix (e.g. `"agents/echo/handle"`).
- Neither method may block — offload to `tokio::spawn`.

### Adding a subsystem

1. Implement `BusHandler` for your subsystem struct.
2. Add it to the `handlers` vec in `main.rs` before calling `supervisor::run`.
3. Announce any new `BusPayload` variants needed and add them to the enum in `bus.rs`.

---

## `LlmProvider` — adding a new model backend

The LLM abstraction uses **enum dispatch** rather than `dyn` trait objects, avoiding `async-trait` and dynamic dispatch overhead.

```rust
pub enum LlmProvider {
    Dummy(providers::dummy::DummyProvider),
    OpenAiCompatible(providers::openai_compatible::OpenAiCompatibleProvider),
}

impl LlmProvider {
    pub async fn complete(&self, content: &str) -> Result<String, ProviderError>;
}
```

### Design rationale

Enum dispatch was chosen over `dyn LlmProvider` because:
- Rust's async/await does not work directly with trait objects without the `async-trait` crate.
- The set of providers is known at compile time and changes infrequently.
- Enum dispatch is zero-cost — no heap allocation or vtable lookup per call.

### Adding a provider

1. Create `src/llm/providers/<name>.rs` with a struct implementing `async fn complete(&self, content: &str) -> Result<String, ProviderError>`.
2. Add a variant to `LlmProvider`.
3. Add a match arm to `LlmProvider::complete`.
4. Add a build case in `src/llm/providers/mod.rs` (`providers::build`).
5. Add configuration fields under `[llm]` in `config/default.toml` and wire them in `config.rs`.

### Current providers

| Variant | Config `provider` value | Status |
|---------|------------------------|--------|
| `Dummy` | `"dummy"` | Implemented — returns `"[echo] {input}"` |
| `OpenAiCompatible` | `"openai"` / `"openai-compatible"` | Implemented — reqwest-based; configurable `api_base_url`, `model`, `temperature`, `timeout_seconds` |

---

## Note on `Channel` and `Component`

Earlier versions of the comms docs described a separate `Channel` trait with `run(self, Arc<CommsState>, CancellationToken)`. The current implementation uses the generic `Component` trait (`run(self: Box<Self>, CancellationToken)`) for all comms channels — `Arc<CommsState>` is captured at construction, not passed to `run`. There is no separate `Channel` trait in the codebase.
