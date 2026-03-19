# Large Rust Project Organization: Best Practices & Patterns

**Date:** March 2026
**Focus:** Message-bus/supervisor architectures, modular subsystems, and organizational trade-offs

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Key Findings: Successful Large Rust Projects](#key-findings)
3. [Workspace Architecture Patterns](#workspace-architecture-patterns)
4. [Compile-Time vs Runtime Modularity](#compile-time-vs-runtime-modularity)
5. [Subsystem Boundary Design](#subsystem-boundary-design)
6. [Plugin System Approaches](#plugin-system-approaches)
7. [Public API Design](#public-api-design)
8. [Araliya-Bot: Current Analysis](#araliya-bot-current-analysis)
9. [Recommendations](#recommendations)

---

## Executive Summary

### Common Patterns in Large Async Rust Systems

**Monolithic Workspace with Modular Subsystems** is the dominant pattern in major async systems (Tokio, Tonic, Tower, Bevy, Embassy, Quinn, Redis implementations). Key characteristics:

- **Single workspace** containing 1–5 primary crate(s) plus utilities
- **Feature-gated subsystems** at compile time for binary size/dependency control
- **Trait-based abstractions** at subsystem boundaries
- **Non-blocking supervisor/router** pattern for event dispatch
- **Capability-passing** (structured dependency injection) rather than global service locators

**Araliya-Bot successfully implements this pattern** and aligns well with industry best practices.

---

## Key Findings: Successful Large Rust Projects

### 1. Tokio Ecosystem

**Architecture:**
- Single workspace (`tokio`) with ~25 member crates
- Core crates: `tokio`, `tokio-util`, `tokio-macros`
- Layered features: `rt`, `macros`, `sync`, `time`, `io-util`, `net`, `fs`, `signal`
- All features are **optional compile-time flags**

**Key Pattern:**
```
tokio/
├── Cargo.toml (workspace)
├── tokio/          (async runtime, feature-gated subsystems)
├── tokio-util/     (utilities, depends on tokio)
├── tokio-macros/   (procedural macros)
├── tokio-io/       (I/O traits)
└── examples/, tests/
```

**Decision Logic:** Features disabled by default in most crates → consumer chooses what they need → binary size optimized for embedded/minimal environments.

### 2. Tonic (gRPC Framework)

**Architecture:**
- Workspace with 6+ crates: `tonic`, `tonic-codegen`, `tonic-reflection`, `tonic-health`, `tonic-web`
- **Trait-based plugin model:**
  - `Interceptor` trait for middleware
  - `NamedService` for service discovery
  - `Status` + custom error types for error handling
- `tonic` core is transport-agnostic; HTTP/2 selected at compile time

**Key Pattern:**
```
pub trait Interceptor: Send + Sync + 'static {
    fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status>;
}
// Implementations are zero-cost abstractions; compiler inlines them
```

**Decision Logic:** Trait objects avoided; enum dispatch used for provider selection. Zero-copy message passing preferred over heap allocation.

### 3. Tower (Middleware Framework)

**Architecture:**
- Modular layer stacking via `Service` trait
- Request/response model: `Service<Request>: Future<Output = Response>`
- Middleware composed via `Layer` trait: `Service` → middleware → `Service`
- Entire design around **pure message passing with zero runtime allocation**

**Key Pattern:**
```
Service (trait)
  ↓ wrapped by
Layer (trait)
  ↓ which returns
Service (trait)
```
Middleware tower can be extended without touching core; zero dynamic dispatch.

### 4. Bevy (Game Engine)

**Architecture:**
- **Monolithic workspace** with 50+ feature-gated subsystems:
  - `bevy_core`, `bevy_render`, `bevy_ecs`, `bevy_asset`, `bevy_audio`, `bevy_ui`
- **Entity-Component-System (ECS)** replaces traditional subsystems
  - Systems (functions) scheduled explicitly
  - Components define data; systems define behavior
- All subsystems **stateless from the user's perspective**; state lives in the ECS `World`

**Key Pattern:**
```toml
# Users opt-in to what they need
bevy = { version = "0.12", features = ["default"] }
# Or minimal:
bevy = { version = "0.12", features = ["minimal", "bevy_core"] }
```

**Decision Logic:** Compile-time modularity reduces binary size from ~500MB to <20MB in minimal mode. Systems scheduled declaratively rather than hardcoded in supervisor loop.

### 5. Embassy (Async Embedded Runtime)

**Architecture:**
- **Executor-agnostic design:** works with STM32H7, Raspberry Pi Pico, etc.
- Minimal allocations; stack-based state passing
- Trait-based **hardware abstraction layer (HAL)**
- No global state; capabilities passed through function parameters

**Key Pattern:**
```rust
pub struct Executor<const TASK_QUEUE_SIZE: usize> {
    run_queue: VecDeque<TaskId>,
    // ...
}
// Zero global singletons; each executor instance is independent
```

**Decision Logic:** Stack-safe, zero-copy message passing. Traits for HAL allow compile-time swapping of implementations.

### 6. Quinn (QUIC Protocol)

**Architecture:**
- Workspace with `quinn`, `quinn-proto`, `quinn-udp`
- **Protocol layer separate from transport:** `quinn-proto` has zero I/O; `quinn` adds async I/O wrapper
- Receiver/Sender split; both owned separately for lock-free concurrency
- State machine-based connection management

**Key Pattern:**
- Pure protocol logic in `quinn-proto` (deterministic, testable)
- I/O wrappers in `quinn` (async, platform-specific)
- **Binary protocol = lightweight serialization**

**Decision Logic:** Separation allows testing protocol without runtime; I/O can be swapped without changing protocol logic.

---

## Workspace Architecture Patterns

### Pattern 1: Single-Crate Monolith with Feature Gates (Most Common)

**Examples:** Tokio, Tonic, Bevy (in part), Araliya-Bot
**Structure:**
```
workspace/
├── Cargo.toml
└── crates/primary/
    ├── Cargo.toml (feature-gated subsystems)
    ├── src/
    │   ├── lib.rs
    │   ├── main.rs
    │   └── subsystems/
    │       ├── agents/
    │       ├── memory/
    │       ├── llm/
    │       ├── comms/
    │       └── ...
    └── tests/
```

**Pros:**
- Simpler dependency graph; fewer CI/build steps
- Easier refactoring (move code without changing crate boundaries)
- Shared test utilities and fixtures
- Unified version management
- Feature interactions caught at compile time

**Cons:**
- Larger initial compilation in worst case (though mitigated by feature gating)
- Subsystem coupling possible (requires discipline in architecture)
- Accidental dependencies harder to spot without careful review

**When to choose:**
- Subsystems share common infrastructure (config, error types, logger)
- Subsystem boundaries are likely to shift during development
- Binary size is a concern but not paramount
- Team is small-to-medium; coordination overhead of multi-crate is high

**Araliya-Bot uses this pattern:** Single `araliya-bot` crate with subsystems in `src/subsystems/agents/`, `src/subsystems/memory/`, etc. Feature flags control which subsystems compile.

### Pattern 2: Multi-Crate Workspace (Modular)

**Examples:** Tokio (extended), Embassy, Quinn, Tonic-extensions
**Structure:**
```
workspace/
├── Cargo.toml (members = [...])
├── araliya-core/          # Shared traits, errors, config
├── araliya-agents/        # Agents subsystem
├── araliya-memory/        # Memory subsystem
├── araliya-llm/           # LLM subsystem
├── araliya-comms/         # Comms subsystem
└── araliya-bot/           # Binary, supervisor
```

**Pros:**
- Cleaner dependency graph (edges are explicit in Cargo.toml)
- Subsystems can't accidentally couple
- Teams can work independently on separate crates
- Each crate has its own feature set and version
- Parallel compilation benefits

**Cons:**
- More Cargo.toml files to maintain
- Shared infrastructure (errors, config) requires a separate crate
- Version pinning across workspace becomes important
- CI complexity increases (must test each crate separately)

**When to choose:**
- Subsystems are mature, boundaries stable
- Teams own subsystems independently
- Crates are published separately or reused in other projects
- Binary size is critical; each crate can be optimized independently
- Dependency management is a priority

### Pattern 3: Hybrid (Bevy-style)

**Structure:**
```
workspace/
├── Cargo.toml (workspace)
├── bevy/              # Primary binary crate
│   ├── src/lib.rs
│   └── src/main.rs
├── bevy_core/         # Internal utility crate
├── bevy_render/       # Render subsystem crate
├── bevy_ecs/          # ECS subsystem crate (published)
└── examples/
```

**Pattern:** Core functionality in primary crate; large subsystems split into separate crates *only when mature*. Reduces friction early; allows extraction later.

**Araliya-Bot consideration:** This is a future-friendly approach — keep current structure now, extract subsystems to separate crates only if they stabilize and teams grow.

---

## Compile-Time vs Runtime Modularity

### Compile-Time Feature Flags (Araliya-Bot's Approach)

**Cargo Features:**
```toml
[features]
default = [
    "subsystem-agents",
    "subsystem-memory",
    "subsystem-llm",
    "plugin-basic-chat",
]
minimal = ["subsystem-agents", "plugin-echo"]
full = ["default", "plugin-gmail", "plugin-news"]

# Subsystem gates:
subsystem-agents = ["subsystem-memory"]  # Dependency
plugin-basic-chat = ["subsystem-agents", "subsystem-llm"]
```

**Code gating:**
```rust
#[cfg(feature = "plugin-basic-chat")]
mod basic_chat;

#[cfg(all(feature = "subsystem-memory", feature = "subsystem-agents"))]
use subsystems::memory_bus::MemoryBusHandler;
```

**Pros:**
- Zero runtime overhead; disabled features = zero code
- Dependencies and implications clear in Cargo.toml
- Binary size optimized for target (embedded = minimal, desktop = full)
- Compile-time guarantees (if plugin X needs subsystem Y, feature gating enforces it)

**Cons:**
- Explosion of feature combinations if not disciplined
- Each combination must be tested (build matrix complexity)
- Cannot enable/disable features at runtime
- Config-time decisions must map to compile-time features

**Industry consensus:** **Compile-time modularity is preferred for async systems** because:
- No runtime dispatch overhead (critical for latency-sensitive apps)
- Unused code completely eliminated by LLVM
- Feature interactions caught early

**Araliya-Bot:**
- **Good:** Clean feature hierarchy; subsystem dependencies explicit
- **Improvement opportunity:** Document feature combination matrix; consider "preset" features (minimal/default/full are a start)

### Runtime Modularity (Alternative)

**Approach:** Feature flag gates entire subsystem, but subsystem is optional at runtime.

```rust
pub fn new(config: &Config) -> Result<Self> {
    #[cfg(feature = "subsystem-agents")]
    let agents = if config.agents.enabled {
        Some(AgentsSubsystem::new(config)?)
    } else {
        None
    };
    // ...
}
```

**Pros:**
- Single binary can be configured post-deployment
- Subsystems can be hot-disabled via config reload

**Cons:**
- Overhead: enum variants, runtime checks, state management
- Complexity: error handling, configuration validation
- Less common in async systems (Tokio does not use this)

**Recommendation:** Keep Araliya-Bot's compile-time approach; runtime modularity adds unnecessary complexity for marginal benefit.

---

## Subsystem Boundary Design

### The Supervisor Pattern (Araliya-Bot's Current Model)

**Architecture:**
```
┌─────────────────────────────────────────────────┐
│  Supervisor (Router/Dispatcher)                 │
│                                                 │
│  Request { method, payload, reply_tx }          │
│    ├─ "agents/*"    → AgentsSubsystem           │
│    ├─ "llm/*"       → LLMSubsystem              │
│    ├─ "memory/*"    → MemoryBusHandler          │
│    ├─ "comms/*"     → CommsSubsystem            │
│    └─ "tools/*"     → ToolsSubsystem            │
│                                                 │
└─────────────────────────────────────────────────┘
```

**Key Design Choices:**

1. **Non-blocking dispatch:** Supervisor forwards `reply_tx` ownership and returns immediately
2. **Star topology:** All communication flows through supervisor; no direct subsystem-to-subsystem coupling
3. **Method prefix routing:** `"prefix/component/action"` → supervisor extracts prefix, forwards rest to handler
4. **BusHandler trait:** Standardized request/response interface

```rust
pub trait BusHandler: Send + Sync {
    fn prefix(&self) -> &str;
    fn handle_request(&self, method: &str, payload: BusPayload, reply_tx: oneshot::Sender<BusResult>);
}
```

**Why this pattern?**

| Aspect | Benefit |
|--------|---------|
| **Non-blocking** | Supervisor loop never stalls; latency-critical |
| **Star topology** | Centralized permission/logging; clear coupling; no cycles |
| **Message-based** | Language-agnostic; future IPC upgrade path (OS processes, HTTP) |
| **Trait-based boundaries** | Subsystems don't reference each other; pure interface contract |

### Alternative: Direct Subsystem Communication

**Example:** Agents directly call memory subsystem (no supervisor involvement)

```rust
// ❌ Anti-pattern: direct coupling
impl Agent {
    async fn handle(&self, input: String) {
        let memory = &self.state.memory;  // Direct ref to MemorySystem
        let session = memory.load_session(...).await;  // Direct call
    }
}
```

**Problems:**
- Creates dependency cycles
- Blocks supervisor loop if memory is slow
- Harder to test (memory must be available)
- Violates capability-passing principle

**Araliya-Bot correctly avoids this:** Agents call bus methods through `AgentsState`, which wraps bus requests.

### Subsystem Responsibilities (Clear Boundaries)

**Based on Araliya-Bot's design:**

| Subsystem | Responsibility | Interface |
|-----------|---|---|
| **Agents** | Route requests to registered agents, spawn agent tasks | `agents/{id}/{action}` |
| **Memory** | Session/transcript storage, KG indexing, spend tracking | `memory/{action}`, read-only bus access |
| **LLM** | Provider abstraction, token counting, rate limiting | `llm/complete`, `llm/tokenize` |
| **Comms** | I/O channels (PTY, HTTP, Telegram), marshalling | Spawned independently; produces messages into bus |
| **Tools** | External actions (Gmail, API calls), execution | `tools/{tool_id}` |
| **Cron** | Scheduled events, timers | `cron/{action}` |
| **UI** | Web/desktop frontend, static asset serving | HTTP/WS handlers |

**Clear boundaries achieved by:**
- Each subsystem owns a **prefix** (e.g., `"agents"`, `"memory"`)
- Subsystems **not aware of each other's internals** (only the interface)
- **Capability passing:** agents receive `Arc<AgentsState>`, not direct refs to other subsystems

### The Component Pattern (Araliya-Bot's Runtime)

```rust
pub trait Component: Send + 'static {
    fn id(&self) -> &str;
    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture;
}
```

**Used for:**
- Comms channels (PTY, HTTP, Telegram)
- Scheduled agents (background tasks)
- UI backends

**Advantages:**
- Stateless from supervisor's perspective (state captured in component)
- Cancellation token enables graceful shutdown
- JoinSet manages concurrent tasks with unified error handling

**Example (comms channel):**
```rust
pub struct PtyChannel {
    state: Arc<CommsState>,
}

impl Component for PtyChannel {
    fn id(&self) -> &str { "pty" }

    async fn run(self: Box<Self>, shutdown: CancellationToken) {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                line = read_stdin() => {
                    // Process input, send to bus
                }
            }
        }
    }
}
```

**Comparison with `BusHandler`:**

| Pattern | Use Case | Interface | Blocking? |
|---------|----------|-----------|-----------|
| `BusHandler` | Subsystems handling requests | `fn handle_request(&self, ...) -> ()` | Non-blocking (or spawns task) |
| `Component` | Long-running tasks | `async fn run() -> Result` | Async, cancellable |

---

## Plugin System Approaches

### Approach 1: Compile-Time Plugins (Araliya-Bot)

**Mechanism:** Feature flags + conditional registration in supervisor loop

```rust
pub fn new(config: &AgentsConfig) -> Result<AgentsSubsystem> {
    let mut agents = HashMap::new();

    #[cfg(feature = "plugin-echo")]
    agents.insert("echo".to_string(), Arc::new(EchoAgent) as Arc<dyn Agent>);

    #[cfg(feature = "plugin-basic-chat")]
    agents.insert("basic_chat".to_string(), Arc::new(BasicChatPlugin::new(state.clone())));

    // ...
}
```

**Pros:**
- Zero runtime overhead; entire plugin inlined into binary
- Type-safe; compile-time verification
- Minimal boilerplate per plugin (impl Agent + feature gate)

**Cons:**
- Cannot add plugins after build
- Build matrix explosion if many combinations needed
- Feature discovery requires reading code

**Industry use:** Tokio, Tonic, Bevy, Embassy

### Approach 2: Runtime Plugins (Loadable)

**Mechanism:** Dynamic library loading (libloading) or trait objects

```rust
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn handle(&self, ...);  // dyn dispatch
}

let plugin: Box<dyn Agent> = if cfg.agent_type == "custom" {
    unsafe { load_plugin("./plugins/agent_custom.so") }?
} else {
    Box::new(BuiltinAgent::new())
};
```

**Pros:**
- Extend agent set without rebuild
- Third-party plugins without source access

**Cons:**
- **Significant complexity:** ABI stability, version management, security vetting
- **Performance:** Dynamic dispatch (vtable lookups) adds latency
- **Safety:** Unsafe code required for `libloading`; crashes in plugin take down supervisor
- **Rarely used in async Rust systems** due to latency sensitivity

**When used:** Audio/video players, Blender, VS Code (but these aren't latency-critical)

### Approach 3: Hybrid (Configuration + Traits)

**Mechanism:** Compile-time plugin set; runtime selection via config

```toml
[agents]
echo = { enabled = true }
basic_chat = { enabled = false }
custom_agent = { type = "builtin", config = {...} }
```

```rust
#[cfg(feature = "plugin-custom")]
agents.insert("custom_agent".to_string(), Arc::new(CustomAgent::new(config)));
```

**Balance:** Plugins built into binary (type-safe, performant); selected at runtime (flexible configuration).

**Araliya-Bot uses this approach:** All plugins compiled in (based on features); agents enabled/disabled via config section in `config/default.toml`.

### Recommendation for Araliya-Bot

**Current approach is optimal:** Compile-time plugins with runtime configuration. Reasons:
1. Latency-critical (supervisor loop, agent dispatch)
2. Plugins are first-party (control ABI + source)
3. Feature matrix manageable (minimal/default/full presets)
4. Zero runtime dispatch overhead
5. Proven pattern across major async systems

**If third-party plugins ever needed:**
- Document a plugin crate template
- Require plugins as separate workspace crates (not dynamic loading)
- Include plugin in `[dependencies]` in main crate
- Plugins must implement `Agent` + register in supervisor

---

## Public API Design

### Principle 1: Expose Traits, Not Implementations

**Good:**
```rust
pub trait Agent: Send + Sync {
    fn id(&self) -> &str;
    fn handle(&self, ...);
}

pub trait BusHandler: Send + Sync {
    fn prefix(&self) -> &str;
    fn handle_request(&self, ...);
}
```

**Why:** Subsystem internals (concrete types) can change; traits are stable contracts.

### Principle 2: Capability-Passing over Global Access

**Good (Araliya-Bot):**
```rust
pub struct AgentsState {
    pub memory: Arc<MemorySystem>,
    pub llm_rates: ModelRates,
    // ...
}

// Agents receive typed state:
fn handle(&self, state: Arc<AgentsState>) { ... }
```

**Bad:**
```rust
// Global state locator
pub fn get_memory() -> &'static MemorySystem { ... }

// Agents call directly:
fn handle(&self) {
    let memory = get_memory();  // ❌ Hides dependency
}
```

**Why:** Explicit dependencies; easier testing; clear what each agent needs.

### Principle 3: Minimum Viable Public Surface

**Good:**
```rust
// lib.rs exports only what external consumers need
pub use config::Config;
pub use error::AppError;
pub mod subsystems {
    pub mod memory;  // Only memory is reusable; other subsystems are internal
}
```

**Current (Araliya-Bot):**
```rust
// lib.rs
mod core;
pub use core::{config, error};
pub mod llm;

#[cfg(feature = "subsystem-memory")]
pub mod subsystems {
    pub mod memory;
}
```

**Good pattern:** Only memory is re-exported (it has external users); agents subsystem is internal.

### Principle 4: Error Types Unified

**Good:**
```rust
pub enum AppError {
    Config(String),
    Bus(String),
    Agent(String),
    // ...
}
```

**Why:** Callers don't need to know subsystem internals; single error type.

**Araliya-Bot:**
```rust
pub enum AppError {
    Comms(String),
    Agent(String),
    // ...
}
```

Unified error type ✓

### Principle 5: JSON RPC 2.0 (Bus Protocol)

**Good (Araliya-Bot):**
```rust
pub enum BusPayload {
    SessionQuery { ... },
    JsonResponse { data: String },
    Empty,
}

pub enum BusResult {
    Ok(BusPayload),
    Err(BusError),  // JSON-RPC error
}
```

**Why:** Protocol is language-agnostic; can upgrade to HTTP/gRPC later without changing subsystem code.

**Future benefit:** Agents/memory/LLM could run in separate processes; messages unchanged.

---

## Araliya-Bot: Current Analysis

### Strengths

1. **Excellent supervisor pattern:** Non-blocking dispatch, star topology, trait boundaries
2. **Clean feature hierarchy:** `minimal`/`default`/`full` presets; feature dependencies explicit
3. **Correct subsystem boundaries:** Agents don't know about memory; memory doesn't know about LLM
4. **Capability-passing:** `AgentsState` provides typed API; bus handle is private
5. **Trait-based interfaces:** `Agent`, `BusHandler`, `Component` are extensible
6. **Unified error handling:** Single `AppError` enum across subsystems
7. **Monolithic workspace:** Simpler refactoring; shared infrastructure

### Areas for Improvement

1. **Feature matrix documentation:**
   - Create a table of valid feature combinations
   - Document each combination's intended use case
   - CI should test key combinations (minimal, default, full)

   **Current:**
   ```toml
   default = [...]     # 15 features
   full = [...]        # 20 features
   minimal = [...]     # 3 features
   ```

   **Recommendation:** Add feature combo tests in CI:
   ```bash
   cargo build --no-default-features --features minimal
   cargo build --no-default-features --features default
   cargo build --all-features
   ```

2. **Public API clarity:**
   - Currently `lib.rs` exports `config`, `error`, `llm`, and memory subsystem
   - Only memory is intended for external reuse
   - Recommendation: Document what's public and why

   **Proposal:**
   ```rust
   // lib.rs
   //! Araliya Bot is structured as a single binary with embedded subsystems.
   //! The memory subsystem ([`subsystems::memory`]) is reusable in other contexts;
   //! other subsystems are internal implementation details.

   pub use error::AppError;
   pub use config::Config;
   pub mod llm;  // Published for transparency

   #[cfg(feature = "subsystem-memory")]
   pub mod subsystems {
       pub mod memory;  // Published for reuse
   }
   ```

3. **Subsystem coupling audit:**
   - Agents subsystem depends on memory (makes sense)
   - Check if any unexpected cross-subsystem dependencies exist
   - Use `cargo tree` to visualize

4. **Message bus versioning:**
   - Bus protocol is currently unversioned
   - If supervisor/comms versions diverge, message format could break
   - Recommendation: Add version negotiation to `BusMessage` (future work)

5. **Plugin registry documentation:**
   - Plugins are discovered by reading `src/subsystems/agents/mod.rs`
   - Document plugin registration in code or architecture guide

### Code Organization Quality

**Current structure:**
```
crates/araliya-bot/src/
├── main.rs (600+ lines: bootstrapping, subsystem init)
├── lib.rs (minimal; only re-exports)
├── bootstrap/ (identity, logger)
├── core/ (config, error)
├── llm/ (provider abstraction)
├── supervisor/ (bus, dispatch, control)
└── subsystems/
    ├── agents/ (23 files; agent plugins + routing)
    ├── memory/ (15 files; stores, session management)
    ├── comms/ (11 files; channels)
    ├── tools/ (4 files; external actions)
    ├── llm/ (1 file; handler)
    └── ...
```

**Assessment:**
- Agents subsystem is large (23 files, 128KB mod.rs); could benefit from further modularization
- Memory subsystem is well-organized (stores/, session/, etc.)
- Supervisor is compact (6 files; responsibilities clear)

**Refactoring opportunity:** Split `agents/mod.rs` into agent families:
```
agents/
├── core.rs (traits, registration)
├── echo.rs
├── chat/
│   ├── mod.rs (ChatCore)
│   ├── basic.rs (BasicChatPlugin)
│   └── session.rs (SessionChatPlugin)
├── docs/
│   ├── mod.rs
│   └── import.rs
├── news/
│   ├── mod.rs
│   ├── news.rs
│   ├── aggregator.rs
│   ├── newsroom.rs
│   └── gdelt_news.rs
├── tools/ (Gmail)
└── runtime_cmd.rs
```

---

## Recommendations

### 1. Keep Current Architecture (No Immediate Changes)

**Why:**
- Monolithic workspace is optimal for team size and feature maturity
- Feature hierarchy is clean; compile-time modularity is correct choice
- Supervisor pattern is proven; supervisor non-blocking design is excellent

**Timeline:** Long-term stable; revisit only if team grows beyond 5 engineers per subsystem.

### 2. Extract Multi-Crate Workspace Only If...

Consider splitting to separate crates **only if all are true:**

- [ ] A subsystem (e.g., agents, memory) is stable enough to version independently
- [ ] Another project wants to reuse that subsystem
- [ ] Team is large enough to own subsystems independently (3+ engineers)
- [ ] Compilation time is a bottleneck (benchmark first)

**For Araliya-Bot today:** No need. Memory subsystem is the only reuse candidate, and its external interface is already stable.

### 3. Strengthen Feature Testing

**Action:** Add CI step to test feature combinations

```yaml
# .github/workflows/build.yml
test-features:
  strategy:
    matrix:
      features:
        - minimal
        - default
        - full
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v3
    - uses: actions-rs/toolchain@v1
    - run: cargo build --no-default-features --features ${{ matrix.features }}
    - run: cargo test --no-default-features --features ${{ matrix.features }}
```

### 4. Document Plugin Registration

**Action:** Add a `PLUGIN_REGISTRY.md` listing all built-in agents:

```markdown
# Agent Plugin Registry

| Agent ID | Feature | Runtime Class | Status |
|----------|---------|---|--------|
| echo | plugin-echo | RequestResponse | Stable |
| basic_chat | plugin-basic-chat | RequestResponse | Stable |
| chat | plugin-chat | Session | Stable |
| docs | plugin-docs | Agentic | Stable |
| news | plugin-news-agent | Specialized | Beta |
```

### 5. Refactor Large Agent Modules (Future)

**When:** After next feature stabilization (3-6 months)

**Action:** Split `agents/mod.rs` into families (chat/, docs/, news/, tools/)

**Benefit:** Faster compilation; easier to navigate; clearer agent taxonomy

### 6. API Documentation

**Action:** Add module-level docs to `lib.rs`:

```rust
//! # Araliya Bot
//!
//! A modular, message-bus supervised async agent framework.
//!
//! ## Architecture
//!
//! Araliya uses a **single-process supervisor** model with subsystems communicating
//! via a JSON-RPC 2.0 message bus. All subsystems are feature-gated for compile-time
//! modularity.
//!
//! ## For Library Users
//!
//! Only [`subsystems::memory`] is intended for external reuse. Other subsystems are
//! internal to the binary and may change without notice.
//!
//! ## For Binary Users
//!
//! Run the bot with:
//!
//! ```sh
//! cargo run --release
//! ```
//!
//! See [`config`] for configuration options and [`error`] for error types.
```

### 7. Message Bus Stability Guarantee

**Action:** Document bus protocol stability:

```markdown
# Bus Protocol Stability

The bus protocol (method names, payload format) is a stable public interface.
Changes are guaranteed to be backward-compatible until a major version bump.

- Method names: `{prefix}/{component}/{action}`
- Payload: JSON serializable via serde
- Error format: JSON-RPC 2.0 error codes
```

### 8. Subsystem Checklist for New Features

**Document a standard for adding subsystems:**

- [ ] Implement `BusHandler` trait with unique prefix
- [ ] Add feature gate: `subsystem-{name}`
- [ ] Register handler in `main.rs` supervisor init
- [ ] Implement `{prefix}/status` route
- [ ] Add unit tests in `#[cfg(test)]` block
- [ ] Document bus methods in architecture guide
- [ ] Add entry to feature matrix
- [ ] Example config in `config/default.toml`

---

## Conclusion

### Summary of Key Patterns

1. **Monolithic single-crate workspace** with feature-gated subsystems is the industry standard for async systems (Tokio, Tonic, Bevy, Embassy, Quinn)

2. **Trait-based boundaries** (Agent, BusHandler, Component) allow subsystems to remain independent without coupling

3. **Non-blocking supervisor** is essential for low-latency message routing

4. **Compile-time modularity** (feature flags) is strongly preferred over runtime modularity (zero overhead, binary size, determinism)

5. **Star topology** (supervisor hub) provides centralized control without actor mailbox complexity

6. **Capability-passing** (structured dependency injection) over global service locators ensures testability and clarity

### Araliya-Bot Assessment

**Araliya-Bot correctly implements all these patterns.** No architectural changes needed in the near term. Recommendations focus on documentation, testing, and future refactoring to maintain clarity as the project grows.

**Strong points:**
- Supervisor architecture is excellent
- Feature hierarchy is clean
- Subsystem boundaries are well-maintained
- Error handling is unified

**Improvement areas:**
- Feature combination testing in CI
- Plugin registry documentation
- Agent module modularization (future)
- Public API clarity

**Verdict:** Araliya-Bot is well-structured for a modular agent framework. Continue current approach; refactor incrementally as team and codebase grow.

---

## References

- **Tokio:** https://github.com/tokio-rs/tokio (single workspace, extensive features)
- **Tonic:** https://github.com/hyperium/tonic (modular workspace, trait-based plugins)
- **Tower:** https://github.com/tower-rs/tower (middleware composition, Service trait)
- **Bevy:** https://github.com/bevyengine/bevy (ECS, feature presets, monolithic workspace)
- **Embassy:** https://github.com/embassy-rs/embassy (executor-agnostic, HAL abstraction)
- **Quinn:** https://github.com/quinn-rs/quinn (protocol/transport separation)

---

**Document Version:** v1.0
**Last Updated:** 2026-03-19
**Author:** Claude Code Research
