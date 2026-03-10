# Architecture Diagrams

This document provides a visual reference for the araliya-bot system architecture.
All diagrams are written in [Mermaid](https://mermaid.js.org/) and render natively
on GitHub and in most Markdown viewers.

---

## How to Draw / Automate Diagrams

Mermaid was chosen for the following reasons:

| Property | Mermaid | Draw.io / Lucidchart | PlantUML |
|---|---|---|---|
| **Version-controlled as text** | ✅ | ❌ (binary XML) | ✅ |
| **Rendered natively on GitHub** | ✅ | ❌ | ❌ |
| **No external tooling to view** | ✅ | ❌ | ❌ |
| **Automatable / codegen-friendly** | ✅ | partial | ✅ |
| **Interactive / styled exports** | partial | ✅ | partial |

To **edit** diagrams: modify this `.md` file directly — GitHub renders them on
the PR/commit view. For local previews, use the
[Mermaid Live Editor](https://mermaid.live) or the VS Code Mermaid extension.

To **export** to SVG/PNG for slides or external documentation, paste a diagram
block into [mermaid.live](https://mermaid.live) and use the download button, or
run the [Mermaid CLI](https://github.com/mermaid-js/mermaid-cli):

```
npx -p @mermaid-js/mermaid-cli mmdc -i diagrams.md -o out/ -e svg
```

To **automate** diagram updates, a future CI step can regenerate diagrams by
rendering all Mermaid blocks in this file to `docs/architecture/generated/`.

---

## Diagram Index

1. [System Overview](#1-system-overview)
2. [Bus Message Protocol](#2-bus-message-protocol)
3. [Startup / Bootstrap Sequence](#3-startup--bootstrap-sequence)
4. [Chat Workflow (end-to-end)](#4-chat-workflow-end-to-end)
5. [Comms Subsystem and Channels](#5-comms-subsystem-and-channels)
6. [Memory System](#6-memory-system)
7. [Identity Hierarchy](#7-identity-hierarchy)
8. [Component Runtime and Fail-fast](#8-component-runtime-and-fail-fast)

---

## 1. System Overview

High-level structure of the single-process supervisor model.  
All subsystems run as Tokio tasks within one OS process and communicate
**exclusively** through the Supervisor Bus (star topology).

```mermaid
graph TB
    subgraph PROCESS["SUPERVISOR PROCESS"]
        direction TB

        subgraph CORE["Core (always on)"]
            CFG["Config\n(TOML + env)"]
            ID["Identity\n(ed25519 keypair)"]
            LOG["Logger\n(tracing)"]
        end

        subgraph BUS["Supervisor Bus  ·  star hub  ·  ~100–500 ns/hop"]
            direction LR
            ROUTER["Typed Channel Router\n(prefix → BusHandler)"]
            CTRL["Control Plane\n(health · status · tree)"]
        end

        subgraph SUBSYSTEMS["Subsystems  (feature-gated Cargo features)"]
            COMMS["Comms\nPTY · HTTP · Telegram"]
            AGENTS["Agents\necho · chat · docs · gmail · news"]
            LLM["LLM\ndummy · OpenAI-compat · Qwen"]
            MEM["Memory\nsessions · collections · docstore"]
            TOOLS["Tools\nGmail · newsmail"]
            CRON["Cron\ninterval · one-shot"]
            UI["UI\nsvui (SvelteKit) · gpui"]
            MGMT["Management\nhealth · status · component tree"]
        end

        CORE --> BUS
        COMMS <-->|bus messages| ROUTER
        AGENTS <-->|bus messages| ROUTER
        LLM <-->|bus messages| ROUTER
        MEM <-->|bus messages| ROUTER
        TOOLS <-->|bus messages| ROUTER
        CRON <-->|notifications| ROUTER
        UI <-->|bus messages| ROUTER
        MGMT <-->|control plane| CTRL
    end

    USER_PTY["User\n(stdin/stdout)"] <-->|text I/O| COMMS
    USER_HTTP["User / Browser\n(HTTP client)"] <-->|REST + WebUI| COMMS
    TG["Telegram\n(teloxide)"] <-->|Telegram API| COMMS
    EXT_LLM["External LLM API\n(OpenAI · Qwen · …)"] <-->|HTTPS| LLM
    EXT_GMAIL["Gmail API\n(OAuth)"] <-->|HTTPS| TOOLS
```

---

## 2. Bus Message Protocol

The bus follows **JSON-RPC 2.0 semantics** in-process. Two message kinds:
- **Request** — caller expects exactly one reply via a `oneshot` channel.
- **Notification** — fire-and-forget; no reply expected.

### 2a. Request / Response Flow

```mermaid
sequenceDiagram
    participant Caller as Caller<br/>(BusHandle)
    participant Bus as Supervisor Bus<br/>(mpsc 64)
    participant Sup as Supervisor<br/>(router loop)
    participant Handler as Target Handler<br/>(e.g. LlmSubsystem)
    participant Worker as Async Worker<br/>(tokio::spawn)

    Caller->>Bus: BusMessage::Request { id, method, payload, reply_tx }
    Note over Caller: awaits oneshot reply_rx
    Bus->>Sup: dequeue message
    Sup->>Handler: handle_request(method, payload, reply_tx)
    Note over Sup: returns immediately — non-blocking
    Handler->>Worker: tokio::spawn { ... }
    Note over Handler: handle_request returns
    Worker->>Worker: do async work (LLM call, DB query, …)
    Worker-->>Caller: reply_tx.send(Ok(BusPayload) | Err(BusError))
    Note over Caller: oneshot resolves
```

### 2b. Notification Flow

```mermaid
sequenceDiagram
    participant Caller as Caller<br/>(BusHandle)
    participant Bus as Supervisor Bus
    participant Sup as Supervisor
    participant Handler as Target Handler

    Caller->>Bus: BusMessage::Notification { method, payload }
    Note over Caller: does NOT await — notify() returns immediately
    Bus->>Sup: dequeue
    Sup->>Handler: handle_notification(method, payload)
    Note over Sup,Handler: lossy under backpressure; no error propagation
```

### 2c. Method Grammar

```mermaid
graph LR
    M["method string"]
    M --> S1["segment 0\n= subsystem prefix\n(dispatch key)"]
    M --> S2["segment 1\n= component / action\n(optional)"]
    M --> S3["segment 2\n= action\n(optional)"]
    S1 -->|examples| EX1["agents\nllm\ncron\ntools\nmanage\nmemory\ncomms"]
    S2 -->|examples| EX2["chat\necho\ncomplete\nschedule"]
    S3 -->|examples| EX3["handle\nexecute"]
```

### 2d. BusPayload Variants

```mermaid
classDiagram
    class BusPayload {
        <<enum>>
        CommsMessage
        LlmRequest
        ToolRequest
        ToolResponse
        CancelRequest
        SessionQuery
        JsonResponse
        CronSchedule
        CronScheduleResult
        CronCancel
        CronList
        CronListResult
        Empty
    }

    class CommsMessage {
        channel_id: String
        content: String
        session_id: Option~String~
        usage: Option~LlmUsage~
    }

    class LlmRequest {
        channel_id: String
        content: String
        system: Option~String~
    }

    class ToolRequest {
        tool: String
        action: String
        args_json: String
        channel_id: String
        session_id: Option~String~
    }

    class CronSchedule {
        target_method: String
        payload_json: String
        spec: CronScheduleSpec
    }

    BusPayload --> CommsMessage
    BusPayload --> LlmRequest
    BusPayload --> ToolRequest
    BusPayload --> CronSchedule
```

---

## 3. Startup / Bootstrap Sequence

Ordered boot steps from `main.rs`.

```mermaid
sequenceDiagram
    participant OS as OS / Shell
    participant Main as main.rs
    participant Cfg as Config Loader
    participant Log as Logger
    participant Id as Identity
    participant Bus as Supervisor Bus
    participant Ctrl as Control Plane
    participant Sub as Subsystems
    participant Sup as Supervisor Loop

    OS->>Main: exec araliya-bot [--interactive] [-v …]
    Main->>Main: dotenvy::dotenv() — load .env (optional)
    Main->>Cfg: config::load(path?) — TOML layers + env overrides
    Cfg-->>Main: AppConfig
    Main->>Log: logger::init(level, log_file)
    Main->>Id: identity::setup(&config)
    Note over Id: scan work_dir for bot-pkey*/<br/>load or generate ed25519 keypair<br/>derive bot_id = SHA256(vk)[..8]
    Id-->>Main: Identity { public_id, identity_dir }
    Main->>Main: CancellationToken::new() — shared shutdown signal
    Main->>Bus: SupervisorBus::new(64) — mpsc bounded 64
    Main->>Ctrl: SupervisorControl::new(32) — control plane
    Main->>Main: tokio::spawn Ctrl-C handler → token.cancel()

    rect rgb(230, 245, 255)
        Note over Main,Sub: Build subsystem handlers (feature-gated)
        Main->>Sub: MemorySystem::new (if subsystem-memory)
        Main->>Sub: LlmSubsystem::new + health_checker (if subsystem-llm)
        Main->>Sub: ToolsSubsystem::new (if subsystem-tools)
        Main->>Sub: AgentsSubsystem::new (if subsystem-agents)
        Main->>Sub: CronSubsystem::new (if subsystem-cron)
        Main->>Sub: ManagementSubsystem::new (always)
    end

    Main->>Sup: tokio::spawn supervisor::run(bus, control, shutdown, handlers)
    Note over Sup: builds prefix→handler table<br/>panics on duplicate prefix
    Main->>Sub: comms::start(config, bus_handle, shutdown) (if subsystem-comms)
    Note over Sub: spawns PTY · HTTP · Telegram channel tasks<br/>runs until shutdown token cancelled
    Main->>Main: shutdown.cancel() + join supervisor
```

---

## 4. Chat Workflow (end-to-end)

Two variants: stateless **basic_chat** and session-aware **chat** (with memory).

### 4a. Stateless Chat (basic_chat agent)

```mermaid
sequenceDiagram
    participant User as User
    participant Chan as Comms Channel<br/>(PTY · HTTP · Telegram)
    participant CS as CommsState
    participant Sup as Supervisor Bus
    participant Agt as AgentsSubsystem<br/>(basic_chat plugin)
    participant CC as ChatCore
    participant LLM as LlmSubsystem
    participant Prov as LLM Provider<br/>(OpenAI · Qwen · dummy)

    User->>Chan: send message text
    Chan->>CS: send_message(channel_id, content, session_id=None)
    CS->>Sup: request("agents", CommsMessage { channel_id, content })
    Sup->>Agt: handle_request("agents", payload, reply_tx)
    Note over Sup: returns immediately
    Agt->>Agt: route by channel_id → basic_chat plugin
    Agt->>CC: ChatCore::basic_complete(state, channel_id, content)
    CC->>Sup: request("llm/complete", LlmRequest { channel_id, content })
    Sup->>LLM: handle_request("llm/complete", payload, reply_tx)
    Note over Sup: returns immediately
    LLM->>Prov: provider.complete(content, system?)
    Prov-->>LLM: LlmResponse { text, usage? }
    LLM-->>CC: reply_tx ← Ok(CommsMessage { channel_id, text, usage })
    CC-->>Agt: CommsMessage { text, usage }
    Agt-->>CS: reply_tx ← Ok(CommsMessage { channel_id, text, usage })
    CS-->>Chan: BusResult
    Chan-->>User: display response text
```

### 4b. Session-Aware Chat (chat agent with memory)

```mermaid
sequenceDiagram
    participant User as User
    participant Chan as Comms Channel
    participant Agt as AgentsSubsystem<br/>(SessionChatPlugin)
    participant Mem as MemorySystem
    participant Sess as SessionHandle<br/>(kv · transcript · spend)
    participant CC as ChatCore
    participant LLM as LlmSubsystem

    User->>Chan: message + optional session_id
    Chan->>Agt: CommsMessage { content, session_id? }
    Agt->>Mem: create_session or load_session(session_id)
    Mem-->>Agt: SessionHandle
    Agt->>Sess: load transcript (last 20 turns)
    Sess-->>Agt: Vec<(role, text)>
    Agt->>Sess: append turn: role=user, content
    Agt->>CC: basic_complete(context built from transcript)
    CC->>LLM: request("llm/complete", LlmRequest { content_with_context })
    LLM-->>CC: LlmResponse { text, usage? }
    CC-->>Agt: response text + usage
    Agt->>Sess: append turn: role=assistant, response_text
    alt usage present
        Agt->>Sess: accumulate_spend(usage, llm_rates)
        Note over Sess: updates spend.json
    end
    Agt-->>Chan: CommsMessage { text, session_id }
    Chan-->>User: response + session_id (for next turn)
```

---

## 5. Comms Subsystem and Channels

The Comms subsystem provides all external I/O. Each channel is an independent
Tokio task using a shared `CommsState` capability boundary.

```mermaid
graph TB
    subgraph COMMS["CommsSubsystem  (feature: subsystem-comms)"]
        CS["CommsState\n(Arc — shared by all channels)\n─────────────────────\nsend_message(ch_id, content, sess_id)\nmanagement_http_get()\nrequest_sessions()\nrequest_session_detail(id)"]

        subgraph CHANNELS["Channels  (each = Box<dyn Component>)"]
            PTY["PTY Channel\npty0, pty1, …\n─────────\nstdin/stdout\nline-based I/O\nauto-disabled in daemon"]
            HTTP["HTTP Channel\n─────────\nAxum server\nbind: 127.0.0.1:8080"]
            VPTY["Virtual PTY\n─────────\nslash protocol\n/chat /health /status /exit"]
            TG["Telegram Channel\n(feature: comms-telegram)\n─────────\nteloxide\nrequires TELEGRAM_BOT_TOKEN"]
        end

        PTY --> CS
        HTTP --> CS
        VPTY --> CS
        TG --> CS
    end

    CS -->|BusHandle::request| BUS["Supervisor Bus"]

    subgraph HTTP_API["HTTP API Routes  (axum router)"]
        R1["GET  /api/health\n→ JSON health snapshot"]
        R2["POST /api/message\n→ forwards to agents\n← MessageResponse { text, session_id }"]
        R3["GET  /api/sessions\n→ session list JSON"]
        R4["GET  /api/session/{id}\n→ metadata + transcript"]
        R5["GET  /api/tree\n→ component tree JSON"]
        R6["GET  /\n→ welcome page or UI delegate"]
    end

    HTTP --> HTTP_API

    USER1["stdin/stdout"] <--> PTY
    USER2["Browser / curl"] <--> HTTP
    USER3["Telegram App"] <--> TG
    SVUI["SvelteKit UI\n(frontend/svui)\nport 5173 (dev)"] <-->|proxied| HTTP
```

---

## 6. Memory System

The Memory subsystem manages sessions, key-value working memory,
transcript history, token spend, and optional document indexing.

```mermaid
graph TB
    subgraph MEM["MemorySystem  (feature: subsystem-memory)"]
        direction TB

        MS["MemorySystem\n─────────────────────────\ncreate_session(store_types, agent)\nload_session(session_id)\nlist_sessions()"]

        subgraph SESS["Session (per conversation)"]
            SH["SessionHandle\n─────────────────────────\nkv_get / kv_set\nappend_turn(role, content)\nget_transcript(last_n)\naccumulate_spend(usage, rates)\nget_spend()"]
            KV["kv.json\n(capped k-v store\nmax kv_cap entries)"]
            TR["transcript.md\n(role: content lines\nmax transcript_cap turns)"]
            SP["spend.json\n(input_tokens, output_tokens\ncost_usd running totals)"]
        end

        subgraph STORES["Session Store Backends"]
            TMP["TmpStore\n(in-memory, no disk)"]
            BASIC["BasicSessionStore\n(~/.araliya/bot-pkey{id}/memory/\nsessions.json index)"]
        end

        subgraph DOCS["Document Stores  (feature-gated)"]
            DS["DocStore\n(feature: idocstore)\n─────────────\nSQLite + FTS5\nBM25 search\nadd_doc, search_chunks"]
            KGD["KG DocStore\n(feature: ikgdocstore)\n─────────────\nentity graph\nBFS from seeds\nedge weighting\nhybrid FTS + KG"]
        end

        MS --> SH
        SH --> KV
        SH --> TR
        SH --> SP
        MS --> TMP
        MS --> BASIC
        MS --> DS
        MS --> KGD
    end

    subgraph DISK["Disk Layout  (~/.araliya/bot-pkey{id}/)"]
        D1["sessions.json\n(index: id · created_at · types · spend)"]
        D2["sessions/{uuid}/kv.json"]
        D3["sessions/{uuid}/transcript.md"]
        D4["sessions/{uuid}/spend.json"]
        D5["agent/{name}-{id}/docstore/chunks.db"]
        D6["agent/{name}-{id}/kgdocstore/\nentities.json · relations.json · graph.json"]
    end

    BASIC --> D1
    BASIC --> D2
    BASIC --> D3
    BASIC --> D4
    DS --> D5
    KGD --> D6
```

---

## 7. Identity Hierarchy

Each bot instance and each named agent has a persistent ed25519 keypair.
`public_id` is derived as `hex(SHA256(verifying_key))[..8]`.

```mermaid
graph TB
    subgraph ID_SETUP["identity::setup()"]
        SCAN["Scan work_dir for bot-pkey*/ directory"]
        LOAD["Load & validate existing keypair"]
        GEN["Generate new keypair → derive public_id"]
        SCAN -->|found| LOAD
        SCAN -->|not found| GEN
    end

    subgraph FS["File System  (~/.araliya/)"]
        BOT_DIR["bot-pkey{8-hex-id}/\n(mode 0700)"]

        subgraph BOT_KEYS["Bot Identity"]
            PRIV["id_ed25519\n(32-byte seed, mode 0600)"]
            PUB["id_ed25519.pub\n(32-byte verifying key, mode 0644)"]
        end

        subgraph MEM_DIR["memory/"]
            SESS_IDX["sessions.json"]
            SESS_DATA["sessions/{uuid}/…"]

            subgraph AGENT_DIR["agent/{name}-{agent-id}/"]
                A_PRIV["id_ed25519\n(agent signing key)"]
                A_PUB["id_ed25519.pub"]

                subgraph SUBAGENT_DIR["subagents/{sub-name}-{sub-id}/"]
                    SA_PRIV["id_ed25519"]
                    SA_PUB["id_ed25519.pub"]
                end
            end
        end

        BOT_DIR --> BOT_KEYS
        BOT_DIR --> MEM_DIR
        MEM_DIR --> SESS_IDX
        MEM_DIR --> SESS_DATA
        MEM_DIR --> AGENT_DIR
        AGENT_DIR --> SUBAGENT_DIR
    end

    ID_SETUP --> FS

    subgraph IDENTITY_STRUCT["Identity struct"]
        F1["public_id: String\n(e.g. '5d16993c')"]
        F2["identity_dir: PathBuf\n(~/.araliya/bot-pkey5d16993c/)"]
        F3["signing_key (private)\nverifying_key (public)"]
    end
```

---

## 8. Component Runtime and Fail-fast

`spawn_components` runs a set of `Component` tasks under a shared
`CancellationToken`. Any component failure cancels all siblings.

```mermaid
flowchart TB
    subgraph SPAWN["spawn_components(components, shutdown)"]
        direction TB

        MGR["Manager Task\n(internal JoinSet)"]

        subgraph TASKS["Tokio Tasks"]
            C1["Component A\n(e.g. HTTP channel)\nrun(shutdown) → Result"]
            C2["Component B\n(e.g. PTY channel)\nrun(shutdown) → Result"]
            C3["Component C\n(e.g. Telegram)\nrun(shutdown) → Result"]
        end

        SH["SubsystemHandle\n─────────────\njoin() → Result\nfrom_handle(JoinHandle)"]

        MGR --> C1
        MGR --> C2
        MGR --> C3
        MGR --> SH
    end

    TOKEN["CancellationToken\n(shared)"]

    C1 <-->|"watches shutdown.cancelled()"| TOKEN
    C2 <-->|"watches shutdown.cancelled()"| TOKEN
    C3 <-->|"watches shutdown.cancelled()"| TOKEN

    FAIL["Component A returns Err(AppError)"]
    CANCEL["token.cancel()\n→ signal siblings"]
    C1 --> FAIL
    FAIL --> CANCEL
    CANCEL --> C2
    CANCEL --> C3

    subgraph COMPONENT_TRAIT["Component trait"]
        TID["id() → &str\n(unique label for logging)"]
        TRUN["run(self, shutdown) → ComponentFuture\n(Pin<Box<dyn Future<Output=Result<(),AppError>>>>)"]
    end
```

---

## Key Types Quick Reference

```mermaid
classDiagram
    class BusHandler {
        <<trait>>
        +prefix() str
        +handle_request(method, payload, reply_tx)
        +handle_notification(method, payload)
        +component_info() ComponentInfo
    }

    class Component {
        <<trait>>
        +id() str
        +run(self, shutdown) ComponentFuture
    }

    class Agent {
        <<trait>>
        +id() str
        +handle(channel_id, content, reply_tx, state)
    }

    class LlmProvider {
        <<enum>>
        Dummy
        OpenAiCompatible
        Qwen
        +complete(content, system) LlmResponse
    }

    class SupervisorBus {
        +handle: BusHandle
        +rx: mpsc::Receiver~BusMessage~
        +new(capacity) SupervisorBus
    }

    class BusHandle {
        +request(method, payload) BusResult
        +notify(method, payload)
        +clone()
    }

    class SessionHandle {
        +kv_get(key) Option~PrimaryValue~
        +kv_set(key, value)
        +append_turn(role, content)
        +get_transcript(last_n) Vec~Turn~
        +accumulate_spend(usage, rates)
        +get_spend() SpendSummary
    }

    class Identity {
        +public_id: String
        +identity_dir: PathBuf
    }

    BusHandler <|.. AgentsSubsystem
    BusHandler <|.. LlmSubsystem
    BusHandler <|.. ToolsSubsystem
    BusHandler <|.. CronSubsystem
    BusHandler <|.. ManagementSubsystem
    BusHandler <|.. MemorySubsystem

    Component <|.. PtyChannel
    Component <|.. HttpChannel
    Component <|.. TelegramChannel

    Agent <|.. EchoAgent
    Agent <|.. BasicChatAgent
    Agent <|.. SessionChatAgent
    Agent <|.. DocsAgent
    Agent <|.. GmailAgent
    Agent <|.. NewsAgent

    SupervisorBus --> BusHandle
    AgentsSubsystem --> BusHandle
    AgentsSubsystem --> Agent
    AgentsSubsystem --> SessionHandle
    SessionHandle --> Identity
```
