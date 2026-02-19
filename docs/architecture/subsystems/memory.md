# Memory Service

**Status:** Planned — not yet implemented.

---

## Overview

The Memory Service is a dedicated subsystem that owns all persistent session data. All other subsystems request memory operations through the supervisor — nothing accesses session files directly.

---

## Responsibilities

- Session lifecycle: create, resolve, list, delete
- Transcript persistence (JSONL, append-only)
- Working memory (per-session markdown scratchpad)
- Observation store (facts, summaries, reflections)
- Usage tracking (token counts, cost estimates)

---

## Data Layout

```
{work_dir}/
└── sessions/
    └── {session_id}/
        ├── transcript.jsonl       append-only, ISO-8601 timestamps
        ├── memory.md              current working state
        ├── observations.jsonl     facts and reflections (future)
        ├── metadata.json          created, mode, model, channel
        └── files/                 attachments and generated artifacts
```

---

## Message Protocol

```
MemoryRequest  →  Memory Service
MemoryResponse ←  Memory Service
```

Key request types (planned):
- `ResolveOrCreateSession`
- `AppendTranscript`
- `LoadTranscript`
- `LoadWorkingMemory` / `SaveWorkingMemory`
- `AppendObservation`
- `RecordUsage`

---

## Concurrency Model

- Lane-based per session: requests for the same session are serialized
- Concurrent across sessions
- Single-writer guarantee prevents transcript corruption
