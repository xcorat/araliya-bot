# Araliya Bot — Complete Documentation Map

**Last Updated:** March 2026 | **Version:** v0.2.0-alpha

This document provides a comprehensive overview of all documentation in the Araliya Bot project. Use this as your master index to navigate the codebase, architecture, and workflows.

---

## 📍 Quick Navigation

### For New Users
Start here if you're new to Araliya Bot:
1. **[Quick Intro](quick-intro.md)** — Understand what Araliya is and how it compares to other systems
2. **[Getting Started](getting-started.md)** — Install, configure, and run your first instance
3. **[Configuration](configuration.md)** — Learn about config files, profiles, and environment variables

### For Architects & Deep Divers
Understand the system design:
1. **[Architecture Overview](architecture/overview.md)** — Design principles, crate workspace, process structure
2. **[Identity System](architecture/identity.md)** — Persistent ed25519 keypairs and public IDs
3. **[Subsystems](architecture/subsystems/)** — LLM, Agents, Comms, Memory, Tools, Cron, UI
4. **[Standards & Protocols](architecture/standards/)** — Bus protocol, capabilities, plugin interfaces, runtime spec

### For Contributors & Developers
Guides for development workflows:
1. **[Contributing Guide](development/contributing.md)** — Code style, workflow, module structure
2. **[Testing](development/testing.md)** — How to run tests, feature flags, test coverage
3. **[Runtimes](development/runtimes.md)** — External runtime execution (Node, Python, Bash)
4. **[GPUI Desktop UI](development/gpui.md)** — Building and running the desktop client

### For Operations & Deployment
Running in production:
1. **[Deployment Guide](operations/deployment.md)** — Docker, environment variables, persistence

---

## 🌳 Full Documentation Structure

### Level 1: Getting Started (Entry Points)

| File | Purpose | Audience |
|------|---------|----------|
| **[index.md](index.md)** | Main entry portal with quick links and feature highlights | Everyone |
| **[quick-intro.md](quick-intro.md)** | 🌸 Project overview, key features, highlights, benchmarks, architecture comparison | New users, evaluators |
| **[getting-started.md](getting-started.md)** | Installation methods, setup wizard, config doctor, feature flags, build from source | New users, developers |
| **[configuration.md](configuration.md)** | Config file format, inheritance, profiles, environment variables, secrets management | Users, operators |

**Key Concepts:**
- Bot acts as an Agentic AI supervisor
- Event-driven, modular architecture
- Persistent ed25519 identity
- Feature-gated subsystems and plugins
- TOML configuration with inheritance

---

### Level 2: Architecture (Technical Deep Dives)

#### Core Architecture
| File | Purpose | Scope |
|------|---------|-------|
| **[overview.md](architecture/overview.md)** | Design principles, crate workspace, process structure, bus protocol overview | System architects |
| **[identity.md](architecture/identity.md)** | Persistent ed25519 keypairs, public IDs, file layout, lifecycle | Identity & security engineering |

#### Subsystems (10 Components)
Located in [architecture/subsystems/](architecture/subsystems/)

| Subsystem | File | Responsibility |
|-----------|------|-----------------|
| **Agents** | `agents.md` | Runtime classes, agent families, orchestration, routing, session queries |
| **Comms** | `comms.md` | I/O channels (PTY, HTTP/Axum, Telegram), message routing |
| **LLM** | `llm.md` | LLM provider abstraction (OpenAI-compatible, Qwen, dummy), token accounting |
| **Memory** | `memory.md` | Session lifecycle, pluggable stores (KV, transcript, SQLite, doc store), bus handler |
| **Tools** | `tools.md` | External tools (Gmail, GDELT BigQuery, RSS), tool invocation, bus protocols |
| **Cron** | `cron.md` | Timer-based scheduling, periodic tasks, bus handler integration |
| **UI** | `ui.md` | Svelte web UI and GPUI desktop UI backends, SPA routing, static file serving |
| **SQLite Store** | `sqlite_store.md` | Hybrid search, structured memory, schema design |
| **Intelligent Doc Store** | `intelligent_doc_store.md` | Semantic memory, vector search, chunking, retrieval |
| **Knowledge Graph DocStore** | `kg_docstore.md` | Graph-based memory, entity extraction, relationship indexing |

#### Standards & Protocols (5 Documents)
Located in [architecture/standards/](architecture/standards/)

| Standard | File | Covers |
|----------|------|--------|
| **Index** | `index.md` | Protocol and interface reference directory |
| **Bus Protocol** | `bus-protocol.md` | Message format, routing, headers, reply channels |
| **Capabilities** | `capabilities.md` | Permission model, capability passing, bus handler access control |
| **Plugin Interfaces** | `plugin-interfaces.md` | Agent, tool, channel, and subsystem plugin API contracts |
| **Runtime Spec** | `runtime.md` | External runtime execution protocol (Node, Python, Bash) |

#### Integrations
| File | Purpose |
|------|---------|
| **[araliya-dht-integration.md](architecture/araliya-dht-integration.md)** | Distributed Hash Table (DHT) integration for network-wide identity and discovery |
| **[diagrams.md](architecture/diagrams.md)** | Visual diagrams of process flow, subsystem interaction, and data flow |

**Key Insights:**
- Single-process supervisor with pluggable subsystems
- Non-blocking event bus (star topology)
- Compile-time modularity via Cargo features
- Multi-tier crate workspace (11 crates total)
- Capability-passing security model

---

### Level 3: Development (Workflows & Practices)

| File | Purpose | Audience |
|------|---------|----------|
| **[contributing.md](development/contributing.md)** | Prerequisites, code style, workflow (check/test/build/run), module conventions | Contributors |
| **[testing.md](development/testing.md)** | Running tests, feature flags, per-crate test breakdown, coverage info | QA, developers |
| **[runtimes.md](development/runtimes.md)** | External runtime execution protocol, Node/Python/Bash integration, example agents | Runtime developers |
| **[gpui.md](development/gpui.md)** | Building the desktop UI, GPUI fundamentals, UI architecture | UI developers |

**Development Essentials:**
- Cargo workspace with 11 crates
- `cargo check` and `cargo test` before commits
- Error handling via `thiserror`, logging via `tracing`
- Feature-gated tests require explicit flags
- One concern per module design principle

---

### Level 4: Operations (Running & Deploying)

| File | Purpose | Audience |
|------|---------|----------|
| **[deployment.md](operations/deployment.md)** | Development setup, Docker containerization, environment variables, persistence | DevOps, operators |

**Deployment Focus:**
- Docker Compose for quick setup
- Environment variable configuration
- Data persistence strategies
- HTTP channel exposure

---

### Level 5: Plans & Roadmap

| File | Purpose |
|------|---------|
| **[2026-03-22-setup-system.md](plans/2026-03-22-setup-system.md)** | System setup plan and roadmap |

---

## 🔄 Documentation Flow

### User Journey: New to Araliya

```
START
  ↓
[Quick Intro] — Understand the project
  ↓
[Getting Started] — Install & configure
  ↓
[Configuration] — Customize settings
  ↓
Run araliya-bot
  ↓
Success? 📊 → [Operations] for deployment
Curious? 🏗️  → [Architecture Overview] for deep understanding
Contributing? 🛠️  → [Contributing Guide] + [Testing]
```

### Developer Journey: Contributing

```
START (in codebase)
  ↓
[Contributing Guide] — Setup & code style
  ↓
[Architecture Overview] — Understand structure
  ↓
[Relevant Subsystem Doc] — Deep dive
  ↓
[Testing] — Add tests
  ↓
Run: `cargo check && cargo test`
  ↓
Commit & submit PR
```

### Operator Journey: Deploying to Production

```
START (deploying)
  ↓
[Getting Started] — Understand config
  ↓
[Configuration] — Setup toml/env
  ↓
[Deployment] — Docker & env vars
  ↓
[Operations] — Monitoring & logs
  ↓
Production ready ✓
```

---

## 📋 Documentation Checklist

### Covered Topics
- ✅ Installation & quickstart
- ✅ Configuration & environment
- ✅ Architecture & design principles
- ✅ Subsystems (10 core components)
- ✅ Protocols & standards
- ✅ Development workflows
- ✅ Testing & CI
- ✅ Deployment & operations
- ✅ Identity & security
- ✅ External runtimes
- ✅ UI frameworks (web & desktop)

### Not Covered (See `/docs/research/`)
- Research-stage designs
- Experimental features
- Long-form technical explorations
- Big Query integration reference
- Workspace organization patterns

---

## 🗂️ File Organization Summary

```
docs/
├── index.md                          # 📍 Main entry point
├── quick-intro.md                    # 🌸 Project overview
├── getting-started.md                # 🚀 Installation guide
├── configuration.md                  # ⚙️  Config reference
├── DOCUMENTATION-MAP.md              # 📖 This file
│
├── architecture/
│   ├── overview.md                   # Core design
│   ├── identity.md                   # ED25519 keypairs
│   ├── diagrams.md                   # Visual references
│   ├── araliya-dht-integration.md     # DHT integration
│   ├── subsystems/                   # 10 subsystem docs
│   │   ├── agents.md
│   │   ├── comms.md
│   │   ├── llm.md
│   │   ├── memory.md
│   │   ├── tools.md
│   │   ├── cron.md
│   │   ├── ui.md
│   │   ├── sqlite_store.md
│   │   ├── intelligent_doc_store.md
│   │   └── kg_docstore.md
│   └── standards/                    # 5 protocol docs
│       ├── index.md
│       ├── bus-protocol.md
│       ├── capabilities.md
│       ├── plugin-interfaces.md
│       └── runtime.md
│
├── development/
│   ├── contributing.md               # Code style & workflow
│   ├── testing.md                    # Test guide
│   ├── runtimes.md                   # External runtimes
│   └── gpui.md                       # Desktop UI
│
├── operations/
│   └── deployment.md                 # Docker & ops
│
├── plans/
│   └── 2026-03-22-setup-system.md    # Roadmap
│
└── research/                         # 📚 Not included in this map
    └── (experimental & reference docs)
```

---

## 🔑 Key Concepts Across Documentation

### Architecture Model
- **Supervisor Model:** Single-process Tokio runtime with pluggable subsystems
- **Event Bus:** Non-blocking star topology for inter-subsystem communication
- **Feature Gates:** Compile-time modularity via Cargo features
- **Capability-Passing:** Subsystems receive only needed handles

### Configuration
- **TOML Format:** Primary config in `config/default.toml`
- **Inheritance:** Config files can extend base files via `[meta] base`
- **Profiles:** Pre-built overlays in `config/profiles/`
- **Secrets:** Separated from config, stored in `.env` (mode 0600)

### Identity
- **ed25519 Keypairs:** Persistent, generated on first run
- **Public ID:** First 8 hex chars of SHA256(verifying_key)
- **Hierarchical:** Bot identity → Agent identity → Subagent identity
- **Storage:** `~/.araliya/bot-pkey{id}/`

### Development
- **Workspace:** 11 independent crates + thin binary
- **Tiers:** Tier 0 (core) → Tier 1 (subsystems) → Tier 2 (agents) → Tier 3 (binary)
- **Features:** Plugin/channel flags forwarded through feature hierarchy
- **Testing:** 318+ tests across workspace, feature-gated test suites

### Deployment
- **Docker:** Full compose setup with volume persistence
- **Environment:** Via `ARALIYA_*` vars and `.env` file
- **Data:** Persistent at `~/.araliya/` (configurable)
- **Logs:** Via `tracing`, configurable via `RUST_LOG`

---

## 📚 Reading Recommendations by Role

### 👤 Bot Operators
1. [Getting Started](getting-started.md)
2. [Configuration](configuration.md)
3. [Deployment](operations/deployment.md)

### 🛠️ Developers / Contributors
1. [Quick Intro](quick-intro.md)
2. [Architecture Overview](architecture/overview.md)
3. [Contributing Guide](development/contributing.md)
4. [Testing](development/testing.md)
5. Relevant subsystem docs as needed

### 🏗️ System Architects
1. [Architecture Overview](architecture/overview.md)
2. [Bus Protocol](architecture/standards/bus-protocol.md)
3. [All Subsystems](architecture/subsystems/)
4. [Capabilities & Security](architecture/standards/capabilities.md)
5. [Identity System](architecture/identity.md)

### 🤖 Agent / Plugin Developers
1. [Architecture Overview](architecture/overview.md)
2. [Agents Subsystem](architecture/subsystems/agents.md)
3. [Plugin Interfaces](architecture/standards/plugin-interfaces.md)
4. [Tools Subsystem](architecture/subsystems/tools.md)
5. [Runtimes](development/runtimes.md)

### 🌐 UI Developers
1. [UI Subsystem](architecture/subsystems/ui.md)
2. [GPUI Guide](development/gpui.md)

### 📡 DevOps / Infrastructure
1. [Deployment](operations/deployment.md)
2. [Configuration](configuration.md)
3. [Architecture Overview](architecture/overview.md) — for understanding subsystems

---

## 🔗 Cross-References

### Configuration References
- Format & syntax → [configuration.md](configuration.md)
- Profile examples → `config/profiles/*.toml`
- Default template → `config/default.toml`
- Overlay/inheritance → [architecture/standards/](architecture/standards/)

### Identity & Security
- Key generation → [identity.md](architecture/identity.md)
- Security model → [capabilities.md](architecture/standards/capabilities.md)
- File layout → [identity.md](architecture/identity.md)

### Subsystem Deep Dives
- Agents runtime → [agents.md](architecture/subsystems/agents.md) + [plugin-interfaces.md](architecture/standards/plugin-interfaces.md)
- Memory stores → [memory.md](architecture/subsystems/memory.md) + `sqlite_store.md` + `intelligent_doc_store.md`
- External tools → [tools.md](architecture/subsystems/tools.md) + [plugin-interfaces.md](architecture/standards/plugin-interfaces.md)
- I/O channels → [comms.md](architecture/subsystems/comms.md) + [plugin-interfaces.md](architecture/standards/plugin-interfaces.md)

### Testing & Quality
- Test running → [testing.md](development/testing.md)
- Code style → [contributing.md](development/contributing.md)
- Feature flags → [getting-started.md](getting-started.md) + [testing.md](development/testing.md)

---

## 📊 Statistics

| Metric | Value |
|--------|-------|
| **Total docs (excluding research)** | 27 files |
| **Main sections** | 5 (Getting Started, Architecture, Development, Operations, Plans) |
| **Subsystems documented** | 10 |
| **Protocol standards** | 5 |
| **Crates in workspace** | 11 |
| **Test coverage** | 318+ tests |
| **Total size** | ~50 KB of core documentation |

---

## 🚀 How to Use This Document

1. **Find Your Role** — See "Reading Recommendations by Role"
2. **Start with Quick Intro** — Understand what Araliya is
3. **Follow Your Path** — Different paths for operators, developers, architects
4. **Drill Down as Needed** — Use subsystem docs for deep understanding
5. **Reference Architecture** — Come back here when lost or context-switching

---

## 📝 Notes for Maintainers

- Keep this map updated when adding /renaming docs
- Sync `reading-recommendations-by-role` when restructuring
- Document new subsystems immediately after adding
- Link to existing files using relative paths: `[filename](path/filename.md)`
- Missing docs? Mark with status badges in table headers
- Research docs intentionally excluded; see `/docs/research/README.md`

---

**Version:** March 2026 | **Last Updated:** [Current Date]
**Araliya Bot v0.2.0-alpha**
