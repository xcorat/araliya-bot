# Source Refactoring Notes

Traced from `main.rs` through imports. Compared against architecture standards (bus-protocol, runtime, plugin-interfaces, capabilities) and Rust best practices.

**Trace order:** main → core/{config, error}, bootstrap/{identity, logger}, llm, supervisor, subsystems → nested modules.

**Updates:**
- **Lint suppressions removed** (2025-02-24): All `#![allow(...)]` and `#[allow(...)]` removed from main.rs, lib.rs, config, control, logger, identity, tools, basic_session, bus, comms, runtime, memory.
- **CLI**: Kept as-is; clap-enabled version planned as a feature later.
- **config refactored into folder** (2025-02-24): `config.rs` → `config/` module. See [Config module layout](#config-module-layout) below.
- **Bootstrap + Core layer implemented** (2026-02-25): Introduced `core/` (config, error) and `bootstrap/` (identity, logger). `llm/` stays at crate root. Crate-root re-exports (`pub use core::{config,error}; pub use bootstrap::{identity,logger}`) preserve all existing `crate::` import paths — no downstream changes required.

---

## 1. main.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| ~~Crate-level `#![allow(dead_code, ...)]`~~ | ~~High~~ | — | **Done:** Removed. |
| `print_startup_summary` in main | Medium | L258 | **Necessary:** Move to a dedicated module (e.g. `startup_summary.rs`) or `cli/mod.rs`. Keeps `main` focused on orchestration. |
| `parse_cli_args` / `CliArgs` in main | Medium | L349–401 | **Necessary:** Move to `cli/mod.rs` or dedicated module. Standard practice: separate CLI parsing from application logic. |
| Manual CLI parsing | Low | L354 | TODO notes clap removal for "lean core". Consider `clap` with `derive` for maintainability; or keep minimal parsing but centralise in one place. |
| ManagementInfo includes LLM fields | High | L147–155 | **Necessary:** Per TODO: LLM provider/config should not be wired in here. Management subsystem should query LLM subsystem for status; do not pass config up from main. |

---

## 2. core/config/ (moved from config/)

### Config module layout

`config` is now a folder with three submodules, living under `core/`:

| File | Purpose |
|------|---------|
| **mod.rs** | Wiring, re-exports (`load`, `load_from`, `expand_home`, all public types), `Config::test_default`, and unit tests. Re-exported at crate root via `pub use core::config`. |
| **types.rs** | Public configuration structs consumed by subsystems: `Config`, `PtyConfig`, `CommsConfig`, `LlmConfig`, `AgentsConfig`, `DocsAgentConfig`, `DocsKgConfig`, `ToolsConfig`, etc. |
| **raw.rs** | Raw TOML deserialization types (`RawConfig`, `RawLlm`, `RawAgents`, …). Private to the config crate. Mirrors the file shape; uses serde `#[serde(default = "fn")]` for missing keys. |
| **load.rs** | Loading logic: `merge_toml`, `load_raw_merged`, `load`, `load_from`, `expand_home`. Converts `RawConfig` → `Config`. |

**Rationale:**
- **types** vs **raw**: Public types are resolved and ready-to-use. Raw types are an implementation detail of TOML parsing. Keeping them separate avoids exposing serde plumbing.
- **load** isolated: Merge, inheritance chains, and env overrides live in one place. Easy to add validation or alternative sources (e.g. remote config) later.
- **Single `mod` entry point**: External code still uses `config::load`, `config::Config`, `config::LlmConfig` — no API changes.

| Issue | Severity | Refactor |
|-------|----------|----------|
| ~~Large file~~ | — | **Done:** Split into config/ folder. |
| Many `Raw*` structs | — | Now in `raw.rs`; acceptable pattern. |

---

## 3. core/error.rs (moved from error.rs)

Re-exported at crate root via `pub use core::error`.

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| Stringly-typed variants | Medium | L6–24 | **Necessary:** Consider wrapping underlying errors with `#[source]` or `#[from]` for better chaining. `AppError::Config(String)` loses original error type. |
| No `source()` impl | Low | — | For `Config`, `Identity`, etc., store underlying error. Improves debugging and `Error::source()` chain. |

---

## 4. bootstrap/identity.rs (moved from identity.rs)

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| Logic complexity | Medium | L57–88 | Per TODO: "not the best logic" — multiple identity dir discovery. Document intended workflow or simplify. Will refine later. |
| Utils location | Low | L148 | TODO: crypto/fs helpers could move to `utils/` or `identity/utils.rs`. |
| `compute_public_id` is `pub` | Low | L156 | Only used internally + tests. Consider `pub(crate)` or move to a test-visible helper. |

---

## 5. bootstrap/logger.rs (moved from logger.rs)

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| None significant | — | — | Clean. |

---

## Module placement — resolved

**Bootstrap + Core layer implemented.** Final layout:

| Module | Location | Notes |
|--------|----------|-------|
| **config** | `core/config/` | Foundational — loaded first. |
| **error** | `core/error.rs` | Cross-cutting, no dependencies. |
| **identity** | `bootstrap/identity.rs` | Run once at startup; consumes `core::config`. |
| **logger** | `bootstrap/logger.rs` | Run once at startup; consumes `core::error`. |
| **llm** | `llm/` (crate root) | Stays at crate root. Future: could move to `providers/llm/` but deferred. |

All four moved modules are re-exported at the crate root so all `crate::config`, `crate::error`, `crate::identity`, `crate::logger` paths continue to resolve without any import-site changes.

---

## 6. llm/ (crate root — unchanged)

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| Duplication with subsystems/llm | Low | — | `crate::llm` = provider abstraction; `subsystems::llm` = bus handler. Separation is intentional. |
| Plugin doc mismatch | Low | — | `plugin-interfaces.md` says `LlmProvider::complete` returns `String`; code returns `LlmResponse`. Update doc. |

---

## 7. supervisor/

### 7.1 mod.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| Direct `table[*k]` indexing | Low | L91 | `table.get(k).unwrap()` style — consider iterator or cleaner access. |
| `serde_json::to_string` unwrap | Medium | L103 | **Necessary:** Use `.unwrap_or_else(|_| "{}".to_string())` or propagate error. Unwrap can panic. |

### 7.2 bus.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| `BusPayload::CommsMessage` has `usage` | Low | L36–41 | bus-protocol.md omits `usage`. **Necessary:** Update spec to include `usage: Option<LlmUsage>`. |
| `#[allow(dead_code)]` on `Notification` | Medium | L170 | Notification path exists but may be underused. Either document intended use or remove if obsolete. |
| Route renaming not documented | Low | — | `llm/complete` was renamed to `llm/self/complete`; update table, note, docs, and consumers accordingly. |
| `#[allow(dead_code)]` on `BusCallError::Full` | Low | L189 | Same — `notify` can return `Full`; ensure callers handle it. |
| `#[allow(dead_code)]` on `notify` | Low | L248 | `notify` is part of public API. Remove allow if it's used. |

> **Note:** the LLM completion route was changed from `llm/complete` to `llm/self/complete`.  Documentation, tests, and any bus clients must be updated to call the new `llm/self/complete` path (and other rename diffs if they arise).  This is the route that the LLM subsystem now exposes for self‑generated completions.

### 7.3 dispatch.rs, control.rs, component_info.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| None critical | — | — | Align with bus-protocol and runtime specs. |

### 7.4 adapters/mod.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| `#[cfg(not(unix))]` dead code | Low | L31–33 | Suppress with `#[allow(dead_code)]` or `cfg`-gate the whole block. |

---

## 8. subsystems/

### 8.1 runtime.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| Duplicated first line | Low | L1 | Fix: "— shared scaffolding for all subsystems." appears twice. |
| `SubsystemHandle::from_handle` `#[allow(dead_code)]` | Low | L71 | Escape hatch; document or remove if unused. |
| Panic mapping to `AppError::Comms` | Medium | L79 | **Necessary:** Joiner panic → `AppError::Comms`. Consider a dedicated variant like `AppError::SubsystemPanic(String)`. |

### 8.2 management/mod.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| LLM info from config | High | L19–24, main L147 | Per capabilities model: management should not hold LLM config. Query via bus or control plane. |
| `cron/list` call when cron disabled | Low | L194 | May fail if cron subsystem not registered. Handled with `_ => vec![]` — acceptable. |

### 8.3 comms/mod.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| Feature-gated channel warnings | — | L115–148 | Good pattern: warn when config enables a channel but feature not compiled. |
| `comms_info.set` | Low | L162 | OnceLock — correct. Ensure `set` is called before any reader. |

### 8.4 agents/mod.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| `_TODO_: fine-grained locks` | Medium | L15 | **Necessary:** Document or implement. Coarse locking can hurt concurrency. |
| Large file (1245 lines) | Medium | — | **Necessary:** Split into submodules (e.g. `agents/state.rs`, `agents/registry.rs`, `agents/routing.rs`). |
| `AgentsState` holds raw `BusHandle` | — | L49 | Per capabilities: bus is private, only typed methods exposed. Correct. |

The docs agent is wired to leverage the memory subsystem for its internal state. Session data is persisted via the session store, every interaction is appended to the transcript log, and document lookups are performed against the memory-backed docstore index. This coupling provides durable session persistence, a searchable conversation history, and efficient access to stored documentation.

### 8.5 memory/mod.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| `now_iso8601` manual implementation | Low | L395–420 | **Standard:** Use `chrono` or `time` crate. Manual date math is error-prone. |
| `sessions_dir` pub via `sessions_root()` | — | L172 | Fine; `sessions_root` is the public API. |

### 8.6 memory/stores/basic_session.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| Magic numbers | Low | L29 | TODO: move to consts or config. |

### 8.7 memory/types.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| TODO about primitives | Low | L36, L176 | Revisit whether wrappers add value vs. using `serde_json::Value` directly. |

### 8.8 tools/gmail.rs

| Issue | Severity | Location | Refactor |
|-------|----------|----------|----------|
| TODO: split setup and gmail | Low | L1 | Refactor into `tools/gmail/` with `mod.rs`, `setup.rs`. |

---

## 9. Standards Compliance Summary

| Standard | Status | Notes |
|----------|--------|-------|
| Bus Protocol (bus-protocol.md) | Mostly | `BusPayload::CommsMessage` has `usage`; doc needs update. Method naming, handlers, errors align. |
| Component Runtime (runtime.md) | Yes | `Component`, `spawn_components`, `SubsystemHandle`, fail-fast behaviour match spec. |
| Plugin Interfaces (plugin-interfaces.md) | Partial | `LlmProvider::complete` returns `LlmResponse` in code, `String` in doc. Agent trait matches. |
| Capabilities (capabilities.md) | Planned | Typed `AgentsState`, `CommsState` in place. Management still receives LLM config from main — violates "subsystem handles own config". |

---

## 10. Cross-Cutting Refactors (Necessary)

1. ~~**Remove crate-level lint suppressions**~~ **Done** (2026-02-24): All `#![allow(...)]` removed from main.rs and lib.rs.
2. **Decouple ManagementInfo from LLM config** (main L147, management L19–24). Management should query subsystems, not receive config.
3. **Extract CLI and startup summary** from main into `cli/` or `startup/` module.
4. **Update bus-protocol.md** to include `usage: Option<LlmUsage>` on `CommsMessage`.
5. **Update plugin-interfaces.md** to reflect `LlmProvider::complete` → `LlmResponse`.
6. **Fix runtime.rs** duplicated first-line in docstring.
7. **Replace manual `now_iso8601`** with `chrono` or `time` (memory/mod.rs).
8. **Consider splitting agents/mod.rs** into smaller modules for maintainability (config is already split into config/).

---

## 11. Optional / Low Priority

- Reintroduce `clap` for CLI if maintenance burden of manual parsing grows.
- `ControlCallError`, `BusCallError`: add `#[non_exhaustive]` if more variants expected.
- `AppError`: add `#[non_exhaustive]` for future error kinds.
- Consolidate TODO/CHECK comments into tracked issues.
