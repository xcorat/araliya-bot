# error Refactor Todo

`error.rs` is now at `core/error.rs` (bootstrap/core layer refactor, 2026-02-25).

Refactor `core/error.rs` into an `error/` folder (like `core/config/`). Two approaches:

---

## Option A: Structural refactor only (core/error.rs → core/error/)

**Scope:** Move file into folder within `core/`, no API changes.

### core/error/ layout
| File | Purpose |
|------|---------|
| **mod.rs** | Re-exports `AppError`, unit tests |
| **app_error.rs** | `AppError` enum definition |

### Files needing changes
**None.** `use crate::error::AppError` keeps working — crate root re-exports `core::error`, which in turn re-exports `AppError`.

### Steps
1. Create `core/error/app_error.rs` with current `AppError` enum
2. Create `core/error/mod.rs` with `pub use app_error::AppError` + tests
3. Delete `core/error.rs`

---

## Option B: Substantive refactor (improve error types)

**Scope:** Per src-refactoring-notes.md — wrap underlying errors with `#[source]`, add `impl From` for domain errors.

### Suggested changes
- `Config(String)` → `Config(#[source] Box<dyn Error>)` or `Config { context: String, source: Option<Box<dyn Error>> }`
- `Identity(String)` → similarly
- Add `impl From<config::LoadError> for AppError`, etc.

### Files needing code changes (27 total)

| File | Changes |
|------|---------|
| **main.rs** | `error::AppError::Memory(e.to_string())` → adapt to new variants |
| **core/config/load.rs** | `AppError::Config(...)` → use new ConfigError or `?` conversion |
| **bootstrap/identity.rs** | `AppError::Identity(...)` → use new IdentityError |
| **bootstrap/logger.rs** | `AppError::Logger(...)` → use new LoggerError |
| **subsystems/agents/mod.rs** | `AppError::Identity`, `AppError::Memory` |
| **subsystems/agents/docs.rs** | various AppError usages |
| **subsystems/agents/docs_import.rs** | AppError usages |
| **subsystems/agents/chat/session_chat.rs** | `crate::error::AppError` |
| **subsystems/memory/mod.rs** | `AppError::Memory` (many sites) |
| **subsystems/memory/handle.rs** | AppError |
| **subsystems/memory/rw.rs** | AppError |
| **subsystems/memory/store.rs** | AppError |
| **subsystems/memory/docstore_manager.rs** | AppError |
| **subsystems/memory/stores/basic_session.rs** | `AppError::Memory` |
| **subsystems/memory/stores/kg_docstore.rs** | `AppError::Memory` |
| **subsystems/memory/stores/docstore.rs** | AppError |
| **subsystems/memory/stores/docstore_core.rs** | AppError |
| **subsystems/memory/stores/agent.rs** | AppError |
| **subsystems/memory/stores/tmp.rs** | AppError |
| **subsystems/runtime.rs** | `AppError::Comms` |
| **subsystems/comms/state.rs** | `AppError::Comms` (many sites) |
| **subsystems/comms/http/mod.rs** | `AppError::Comms` |
| **subsystems/comms/http/ui.rs** | AppError |
| **subsystems/comms/http/api.rs** | `AppError::Comms` |
| **subsystems/comms/axum_channel/mod.rs** | `AppError::Comms` |
| **subsystems/comms/pty.rs** | AppError |
| **subsystems/comms/telegram.rs** | AppError |

---

## Recommendation

- **Do Option A first** — low risk, keeps consistency with config/ layout.
- **Option B later** — plan as a separate PR; 27 files is a sizable refactor. Will refine approach later.
