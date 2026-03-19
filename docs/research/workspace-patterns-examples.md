# Workspace Organization Patterns: Detailed Examples

**Reference guide** showing how 6 major Rust projects organize their workspaces.

---

## 1. Tokio — Single Crate with Many Features

**Philosophy:** Opt-in modularity. Core runtime is lightweight; users add only what they need.

**Structure:**
```
tokio/
├── Cargo.toml (workspace root)
│   └── members = ["tokio", "tokio-util", "tokio-macros"]
│
├── tokio/ (core runtime crate)
│   ├── Cargo.toml
│   │   └── [features]
│   │       default = ["macros", "rt-multi-thread", "signal", ...]
│   │       minimal = ["rt"]
│   │       full = [all of above + "tracing", "stats", ...]
│   │
│   └── src/
│       ├── lib.rs
│       ├── task/ (🔧 feature-gated)
│       ├── time/ (🔧 feature-gated)
│       ├── sync/ (🔧 feature-gated)
│       ├── io/ (🔧 feature-gated)
│       └── net/ (🔧 feature-gated)
│
├── tokio-util/ (utilities, soft dependency)
│   └── depends on tokio
│
└── tokio-macros/ (macros)
    └── procedural macro definitions
```

**Feature Example:**
```toml
# tokio/Cargo.toml
[features]
default = ["macros", "rt-multi-thread"]
macros = ["tokio-macros"]
rt = []
rt-multi-thread = ["rt"]
signal = ["rt"]
time = ["rt"]
io-util = ["io-std"]
io-std = ["rt"]
net = ["rt"]
fs = ["rt"]
sync = ["rt"]

# No embedded subsystem has its own feature gate;
# each enables "rt" as a dependency:
# e.g., time = ["rt"]
```

**Key insight:** Even with many subsystems, feature complexity is managed via **declared dependencies** (e.g., `time = ["rt"]` means enabling timer support requires runtime support).

---

## 2. Tonic — Modular Workspace with Separate Crates

**Philosophy:** Each capability is a separate crate. Core is tiny; extensions are optional.

**Structure:**
```
tonic/
├── Cargo.toml (workspace root)
│   └── members = [
│         "tonic",
│         "tonic-codegen",
│         "tonic-reflection",
│         "tonic-health",
│         "tonic-web",
│         "tonic-build",
│       ]
│
├── tonic/ (core gRPC framework)
│   ├── Cargo.toml
│   │   └── [features]
│   │       default = ["transport"]
│   │       transport = ["axum", "tokio", "h2", "hyper"]
│   │       compression = ["flate2", "gzip"]
│   │
│   └── src/
│       ├── lib.rs
│       ├── status.rs (tonic::Status error type)
│       ├── service.rs (Service trait and impl)
│       ├── metadata.rs (gRPC metadata/headers)
│       └── transport/ (🔧 feature-gated)
│
├── tonic-codegen/ (code generation)
│   ├── prost-derive-powered code gen
│   └── Not enabled by default; only at build time
│
├── tonic-reflection/ (service reflection)
│   └── Separate crate; feature in main tonic
│
└── tonic-health/ (gRPC health checks)
    └── Separate crate; feature in main tonic
```

**Cargo.toml example:**
```toml
# tonic/Cargo.toml
[package]
name = "tonic"

[dependencies]
tokio = { version = "1", optional = true }
axum = { version = "0.6", optional = true }
h2 = { version = "0.3", optional = true }
hyper = { version = "0.14", optional = true }

[features]
default = ["transport", "codegen"]
transport = ["tokio", "axum", "h2", "hyper"]
codegen = ["tonic-codegen"]  # Build-time only

[dev-dependencies]
tonic-reflection = { path = "../tonic-reflection" }
```

**Key insight:** Each independently-useful capability (reflection, health, web) lives in a separate crate. Core `tonic` has no opinion on how you use it.

---

## 3. Quinn — Protocol/Transport Separation

**Philosophy:** Pure protocol logic separate from I/O; each swappable independently.

**Structure:**
```
quinn/
├── Cargo.toml (workspace)
│   └── members = ["quinn", "quinn-proto", "quinn-udp"]
│
├── quinn-proto/ (pure protocol, NO I/O)
│   ├── Cargo.toml
│   │   └── dependencies: ring, bytes, serde (NO tokio)
│   │
│   └── src/
│       ├── lib.rs
│       ├── connection.rs (state machine)
│       ├── crypto.rs (TLS)
│       ├── frame.rs (packet format)
│       └── // Zero I/O code
│
├── quinn-udp/ (UDP transport layer)
│   ├── Cargo.toml
│   │   └── dependencies: tokio, quinn-proto
│   │
│   └── src/
│       ├── unix.rs (platform-specific)
│       ├── windows.rs (platform-specific)
│       └── abstract_socket.rs
│
└── quinn/ (async I/O wrapper)
    ├── Cargo.toml
    │   └── dependencies: tokio, quinn-proto, quinn-udp
    │
    └── src/
        ├── lib.rs
        ├── endpoint.rs (create/receive connections)
        ├── connection.rs (async send/recv wrapper)
        ├── stream.rs (Stream trait)
        └── // High-level async API
```

**Benefits:**
1. **Testability:** `quinn-proto` tested deterministically (no async, no I/O)
2. **Swappability:** Replace `quinn-udp` with custom transport without touching protocol logic
3. **Modularity:** Each crate has one responsibility

**Dependency flow:**
```
quinn (async I/O)
  ↓
quinn-proto (protocol)  ← testable in isolation
quinn-udp (UDP I/O)     ← swappable

# Consumer usage:
quinn::Endpoint {        // Async API
  proto: quinn-proto::Connection,  // Pure logic
  io: quinn-udp::Socket,          // Platform I/O
}
```

**Key insight:** Separate layers allow each to be tested, optimized, and replaced independently.

---

## 4. Bevy — Feature-Gated Subsystems in Single Crate

**Philosophy:** Monolithic crate with aggressive feature flags. Reduce bloat via opt-in features.

**Structure:**
```
bevy/
├── Cargo.toml (workspace)
│   └── members = ["bevy", "bevy_ecs", "bevy_asset", ...]
│
├── bevy/ (primary binary/library crate)
│   ├── Cargo.toml
│   │   ├── [features]
│   │   │   default = ["core", "render", "ui", "input"]
│   │   │   minimal = ["bevy_core"]
│   │   │   all = ["default", "audio", "animation", ...]
│   │   │
│   │   └── [dependencies]
│   │       bevy_core = { version = "0.12", optional = true }
│   │       bevy_render = { version = "0.12", optional = true }
│   │       bevy_ecs = { version = "0.12", optional = true }
│   │
│   └── src/
│       ├── lib.rs
│       ├── prelude.rs (re-exports for convenience)
│       │
│       ├── core/ (🔧 feature-gated "core")
│       │   ├── entity.rs
│       │   ├── world.rs
│       │   └── ...
│       │
│       ├── render/ (🔧 feature-gated "render")
│       │   ├── texture.rs
│       │   ├── camera.rs
│       │   ├── mesh.rs
│       │   └── ...
│       │
│       ├── ui/ (🔧 feature-gated "ui")
│       │   ├── widget.rs
│       │   ├── layout.rs
│       │   └── ...
│       │
│       └── // Each subsystem gated independently
│
└── bevy_ecs/ (published as standalone crate)
    ├── Cargo.toml
    └── src/
        ├── system.rs
        ├── query.rs
        └── // Self-contained ECS
```

**Feature configuration example:**
```toml
# bevy/Cargo.toml
[features]
# Granular features:
bevy_core = ["bevy_core_crate"]
bevy_render = ["bevy_render_crate", "bevy_core"]
bevy_ui = ["bevy_ui_crate", "bevy_render"]
bevy_audio = ["bevy_audio_crate"]

# Convenience presets:
default = ["bevy_core", "bevy_render", "bevy_ui", "bevy_input"]
minimal = ["bevy_core"]
all = ["default", "bevy_audio", "bevy_animation", ...]
```

**User configuration:**
```rust
// Option A: Minimal (ECS only, ~20MB binary)
use bevy::prelude::*;

// Option B: Default (core + render + UI, ~100MB)
cargo build --release

// Option C: Custom
cargo build --no-default-features --features bevy_core,bevy_ecs
```

**Key insight:** Feature flags can be organized in layers (render depends on core; UI depends on render). This forces a natural dependency hierarchy and prevents circular dependencies.

---

## 5. Embassy — Modular Stack with HAL Abstraction

**Philosophy:** Minimal core runtime; HAL traits allow swapping device implementations.

**Structure:**
```
embassy/
├── Cargo.toml (workspace)
│   └── members = [
│         "embassy-executor",
│         "embassy-time",
│         "embassy-nrf",      # Nordic Semiconductor HAL
│         "embassy-stm32",    # STMicroelectronics HAL
│         "embassy-rp",       # Raspberry Pi Pico HAL
│         "embassy-usb",
│         ...
│       ]
│
├── embassy-executor/ (abstract executor)
│   ├── Cargo.toml
│   │   └── [features]
│   │       default = []
│   │       std = ["std library support"]
│   │       wasm = ["wasm-bindgen"]
│   │       cortex-m = ["cortex-m crate"]
│   │
│   └── src/
│       ├── lib.rs
│       ├── arch/ (platform-specific: cortex-m, std, wasm)
│       └── raw/ (core executor logic)
│
├── embassy-time/ (timer abstraction)
│   ├── Cargo.toml
│   └── src/
│       ├── driver.rs (trait: TimerDriver)
│       └── // Zero platform code
│
├── embassy-hal-common/ (shared HAL traits)
│   └── src/
│       ├── uart.rs (UART trait)
│       ├── spi.rs (SPI trait)
│       ├── gpio.rs (GPIO trait)
│       └── // Protocol definitions, no impl
│
├── embassy-nrf/ (Nordic Semiconductor)
│   ├── Cargo.toml
│   │   └── [features]
│   │       nrf52840 = [...]
│   │       nrf5340 = [...]
│   │
│   └── src/
│       ├── uart/ (impl UART for nRF)
│       ├── spi/ (impl SPI for nRF)
│       ├── gpio/ (impl GPIO for nRF)
│       └── ...
│
└── embassy-stm32/ (STMicroelectronics)
    ├── Cargo.toml
    │   └── [features] (different STM32 variants)
    └── src/
        ├── uart/ (impl UART for STM32)
        ├── spi/ (impl SPI for STM32)
        ├── gpio/ (impl GPIO for STM32)
        └── ...
```

**Trait-based abstraction example:**
```rust
// embassy-hal-common/src/uart.rs
pub trait Uart {
    async fn write(&mut self, buf: &[u8]) -> Result<(), UartError>;
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, UartError>;
}

// embassy-nrf/src/uart.rs
pub struct UartNrf { /* ... */ }
impl Uart for UartNrf { /* ... */ }

// embassy-stm32/src/uart.rs
pub struct UartStm32 { /* ... */ }
impl Uart for UartStm32 { /* ... */ }

// User code (generic over HAL):
async fn read_sensor<U: Uart>(uart: &mut U) -> Result<u8, UartError> {
    let mut buf = [0u8];
    uart.read(&mut buf).await?;
    Ok(buf[0])
}
```

**Key insight:** Traits define hardware semantics; each platform provides implementations. User code is platform-agnostic.

---

## 6. Araliya-Bot — Single Crate with Feature-Gated Subsystems

**Philosophy:** All code in one binary crate; feature flags select subsystems at compile time.

**Structure:**
```
araliya-bot/
├── Cargo.toml (workspace root)
│   └── members = ["crates/araliya-bot"]
│
└── crates/araliya-bot/
    ├── Cargo.toml
    │   ├── [features]
    │   │   # Subsystem gates:
    │   │   subsystem-agents = ["subsystem-memory"]
    │   │   subsystem-memory = []
    │   │   subsystem-llm = []
    │   │   subsystem-comms = []
    │   │   subsystem-ui = []
    │   │
    │   │   # Agent plugin gates:
    │   │   plugin-echo = ["subsystem-agents"]
    │   │   plugin-basic-chat = ["subsystem-agents", "subsystem-llm"]
    │   │   plugin-chat = ["subsystem-agents", "subsystem-memory", "subsystem-llm"]
    │   │   plugin-docs = ["subsystem-agents", "idocstore"]
    │   │   plugin-gmail-agent = ["subsystem-agents", "subsystem-tools"]
    │   │   plugin-news-agent = ["subsystem-agents", "subsystem-tools"]
    │   │
    │   │   # Channel gates:
    │   │   channel-pty = ["subsystem-comms"]
    │   │   channel-axum = ["subsystem-comms", "dep:axum"]
    │   │   channel-telegram = ["subsystem-comms", "dep:teloxide"]
    │   │
    │   │   # UI gates:
    │   │   ui-svui = ["subsystem-ui"]
    │   │   ui-gpui = ["dep:gpui"]
    │   │
    │   │   # Presets:
    │   │   default = [
    │   │     "subsystem-agents", "subsystem-memory", "subsystem-llm",
    │   │     "plugin-basic-chat", "plugin-chat",
    │   │     "channel-pty", "channel-axum", "ui-svui"
    │   │   ]
    │   │   minimal = ["subsystem-agents", "subsystem-llm", "channel-pty", "plugin-basic-chat"]
    │   │   full = ["default", "plugin-gmail-agent", "plugin-news-agent", ...]
    │   │
    │   └── [dependencies] (optional)
    │       axum = { version = "0.8", optional = true }
    │       teloxide = { version = "0.13", optional = true }
    │       gpui = { version = "0.2.2", optional = true }
    │
    ├── src/
    │   ├── lib.rs (exposes config, error, subsystems)
    │   ├── main.rs (supervisor init)
    │   │
    │   ├── bootstrap/ (identity, logger)
    │   ├── core/ (config, error types)
    │   ├── llm/ (provider abstraction)
    │   ├── supervisor/ (bus, dispatch, control)
    │   │   ├── bus.rs (JSON-RPC protocol)
    │   │   ├── dispatch.rs (BusHandler trait)
    │   │   ├── component_info.rs (management tree)
    │   │   └── health.rs (health reporters)
    │   │
    │   └── subsystems/ (feature-gated)
    │       ├── runtime.rs (Component trait, spawn_components)
    │       ├── agents/ (23 files; routes to agents)
    │       │   ├── mod.rs (agent registry, routing)
    │       │   ├── core.rs (Agent trait)
    │       │   ├── echo.rs (#[cfg(feature = "plugin-echo")])
    │       │   ├── chat/
    │       │   │   ├── core.rs (ChatCore composition)
    │       │   │   ├── basic.rs (BasicChat)
    │       │   │   └── session.rs (SessionChat)
    │       │   ├── docs/ (DocsAgent)
    │       │   ├── news/ (news, gdelt, newsroom)
    │       │   ├── gmail.rs (GmailAgent)
    │       │   └── // Each behind feature gate
    │       │
    │       ├── memory/ (15 files; session/transcript/KG stores)
    │       │   ├── mod.rs (MemorySystem, stores)
    │       │   ├── stores/
    │       │   │   ├── session.rs
    │       │   │   ├── transcript.rs
    │       │   │   ├── docstore.rs (#[cfg(feature = "idocstore")])
    │       │   │   └── kg_docstore.rs (#[cfg(feature = "ikgdocstore")])
    │       │   └── // MemorySystem is public (subsystems::memory)
    │       │
    │       ├── comms/ (11 files; I/O channels)
    │       │   ├── mod.rs (CommsSubsystem)
    │       │   ├── pty.rs (#[cfg(feature = "channel-pty")])
    │       │   ├── axum.rs (#[cfg(feature = "channel-axum")])
    │       │   └── telegram.rs (#[cfg(feature = "channel-telegram")])
    │       │
    │       ├── llm/ (handler; trait in core/llm)
    │       ├── tools/ (external actions)
    │       ├── cron/ (scheduler)
    │       ├── ui/ (frontend server)
    │       └── runtimes/ (runtime execution)
    │
    └── bin/
        ├── araliya-bot (main binary)
        ├── araliya-ctl (#[cfg(feature = "cli")])
        ├── araliya-gpui (#[cfg(feature = "ui-gpui")])
        └── araliya-beacon (#[cfg(feature = "ui-beacon")])
```

**Feature dependency graph:**
```
subsystem-agents
  ├── subsystem-memory (required)
  └── plugins:
      ├── plugin-echo
      ├── plugin-basic-chat (requires subsystem-llm)
      ├── plugin-chat (requires subsystem-memory, subsystem-llm)
      └── plugin-docs (requires idocstore)

channel-axum
  └── subsystem-comms (required)
  └── dep:axum (required)

default = [subsystem-agents, subsystem-llm, channel-pty, ...]
minimal = [subsystem-agents, subsystem-llm, channel-pty, plugin-basic-chat]
full = [default + all plugins]
```

**Key insight:** Feature dependencies are explicit in Cargo.toml. Compiler enforces consistency (e.g., `plugin-basic-chat` requires `subsystem-llm`).

---

## Comparison Table

| Project | Model | Workspace | Subsystem Separation | Plugin Model | Best For |
|---------|-------|-----------|----------------------|--------------|----------|
| **Tokio** | Feature gates | Single crate + utils | Feature-based | Compile-time | Async runtime users; lightweight core |
| **Tonic** | Multi-crate | 6+ crates | Separate crates | Traits + composition | gRPC servers; modular design |
| **Quinn** | Layered | Proto + Transport + I/O | Layer-based | Traits (protocol/transport split) | Protocol implementations; testability |
| **Bevy** | Feature gates | Primary crate + ECS | Feature-based + ECS | Compile-time systems | Game engines; complex subsystems |
| **Embassy** | Trait-based HAL | Multi-crate | Trait implementation | Traits (HAL) | Embedded systems; hardware abstraction |
| **Araliya-Bot** | Feature gates | Single crate | Feature-based | Compile-time agents | Agent frameworks; modular bots |

---

## Decision Matrix: Which Pattern Should You Use?

**Choose Tokio-style (single crate, many features) if:**
- Subsystem boundaries may shift
- Team is small (< 3 per subsystem)
- Binary size matters more than separation
- Subsystems share infrastructure
- Rapid refactoring expected

✅ **Araliya-Bot is here**

---

**Choose Tonic-style (multi-crate) if:**
- Subsystems are mature and stable
- Teams own subsystems independently
- Crates are published separately
- Clear external interfaces
- API stability is critical

🔮 **Araliya-Bot could move here if agent library matures**

---

**Choose Quinn-style (layered crates) if:**
- Pure logic (testable) vs I/O (platform-specific)
- Protocol/transport/UI separation critical
- Different teams work on different layers
- Swappability is important

---

**Choose Bevy-style (feature-heavy single crate) if:**
- Many subsystems with optional features
- Aggressive binary size optimization
- Complex feature interdependencies
- Some subsystems published separately (ECS)

---

**Choose Embassy-style (trait-based HAL) if:**
- Hardware abstraction is central
- Multiple implementations per interface
- Platform-specific code is significant
- Generic user code needed

---

## Conclusion

**Araliya-Bot has chosen correctly:** Single-crate with feature-gated subsystems and trait-based boundaries. This strikes the right balance between modularity (clear subsystem interfaces) and simplicity (one compilation unit, shared infrastructure).

**Future migration path:**
1. **Phase 1 (now):** Feature-gated single crate (current)
2. **Phase 2 (6–12 mo):** Extract memory subsystem to separate crate if external users emerge
3. **Phase 3 (12+ mo):** Extract agents to separate crate if agent library matures
4. **Phase 4 (optional):** Multi-crate workspace with agent-library as published crate

This gradual approach avoids over-engineering early while leaving a clear path forward.

---

**Document Version:** v1.0
**Last Updated:** 2026-03-19
