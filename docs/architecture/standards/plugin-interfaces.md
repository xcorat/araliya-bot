# Plugin Interfaces

**Status:** Implemented — agents (`src/subsystems/agents/mod.rs`), LLM (`src/llm/mod.rs`), BusHandler (`src/supervisor/dispatch.rs`)

This document specifies the three extension points in the architecture: how to add a new agent plugin, a new LLM provider, and how subsystems register on the bus.

---

## `AgentPlugin` — adding a new agent

```rust
pub trait AgentPlugin: Send + Sync {
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

- `id()` must match the name used in `[agents].enabled` config and `[agents.channel_map]` values.
- `handle` must **not block the caller**:
  - Synchronous plugins resolve `reply_tx` immediately (see `EchoPlugin`).
  - Async plugins `tokio::spawn` a task and resolve `reply_tx` from within it (see `BasicChatPlugin`).
- `reply_tx` is `oneshot::Sender<BusResult>` — consume it exactly once. Dropping it without sending causes the caller to receive `BusCallError::Recv`.
- `state: Arc<AgentsState>` is the capability surface. Do not circumvent it to access raw bus handles.

### `AgentsState` capability surface

```rust
impl AgentsState {
    pub async fn complete_via_llm(&self, channel_id: &str, content: &str) -> BusResult;
}
```

Plugins call typed methods on `AgentsState` rather than addressing arbitrary bus targets. The raw `BusHandle` is private to the agents module.

### Built-in plugins

| ID | Behaviour |
|----|-----------|
| `echo` | Returns input unchanged; synchronous. |
| `basic_chat` | Calls `complete_via_llm` in a `tokio::spawn` task. |

### Adding a plugin

1. Implement `AgentPlugin` in a new file under `src/subsystems/agents/`.
2. Register it in `AgentsSubsystem::new()` (the `plugins` `HashMap`).
3. Add the plugin ID to `[agents].enabled` in `config/default.toml`.

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
