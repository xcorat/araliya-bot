# Intelligent Knowledge-Graph Document Store (IKGDocStore)

**Status:** Phase 1 (2026-02-23) — Feature-gated KG-augmented document store · same base API as `IDocStore` · offline KG build pipeline · BFS graph traversal at query time · KG+FTS merged context assembly · pure FTS fallback · configurable tuning parameters.

**Cargo Feature:** `ikgdocstore`

> **See also:** [intelligent_doc_store.md](intelligent_doc_store.md) — `IDocStore`, the BM25-only sibling store and `docstore_core` shared types.

---

## Overview

`IKGDocStore` is a self-contained document store that layers a knowledge graph on top of the same chunk-indexed SQLite backend used by `IDocStore`.  It is designed for agents that need richer retrieval context than BM25 alone can provide.

The **KG-RAG pipeline** splits into two offline/online phases:

| Phase | When | What |
|-------|------|------|
| **Build** (`rebuild_kg`) | After document import | Extract entities + relations from all chunks; write `kg/` JSON files. |
| **Query** (`search_with_kg`) | Per user request | Load graph, match seed entities from the prompt, BFS-traverse, merge with FTS, assemble LLM context. |

`IDocStore` is not modified.  Both stores can coexist in the same agent identity directory (`docstore/` vs `kgdocstore/`).

---

## Storage Layout

```
{agent_identity_dir}/
└── kgdocstore/
    ├── chunks.db                  SQLite — identical schema to IDocStore
    ├── docs/
    │   └── {doc_id}.txt           raw document content
    └── kg/
        ├── entities.json          entity map (id → Entity)
        ├── relations.json         relation list
        └── graph.json             combined — fast-load file used at query time
```

---

## KG Types

### `EntityKind`

```rust
pub enum EntityKind { Concept, System, Person, Term, Acronym }
```

| Variant | Extracted from |
|---------|---------------|
| `Term` | Backtick-quoted (`` `Foo` ``) or double-quoted (`"Foo"`) spans |
| `System` | CamelCase identifiers (`AuthService`, `TokenCache`) |
| `Concept` | Title Case noun phrases of ≥ 2 words not at a sentence start |
| `Acronym` | 2–5 ALL-CAPS letters (`API`, `RAG`, `LLM`) |
| `Person` | Injected via the `domain_seeds` parameter |

### `Entity`

```rust
pub struct Entity {
    pub id: String,            // 16-hex-char prefix of sha256(name)
    pub name: String,          // normalised (lowercase)
    pub kind: EntityKind,
    pub mention_count: usize,  // sum of per-chunk occurrences
    pub source_chunks: Vec<String>,
}
```

### `Relation`

```rust
pub struct Relation {
    pub from: String,          // Entity::id
    pub to: String,            // Entity::id
    pub label: String,         // "uses" | "implements" | "co-occurs" | …
    pub weight: f32,           // normalised to (0, 1]
    pub source_chunks: Vec<String>,
}
```

### `KgGraph`

```rust
pub struct KgGraph {
    pub entities: HashMap<String, Entity>,  // id → Entity
    pub relations: Vec<Relation>,
}
```

Serialised to `kg/graph.json` after every `rebuild_kg` call.

---

## `KgConfig` — Tuning Parameters

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `min_entity_mentions` | `usize` | `2` | Discard entities with fewer total mentions across all chunks. |
| `bfs_max_depth` | `usize` | `2` | BFS hop limit from seed entities. |
| `edge_weight_threshold` | `f32` | `0.15` | Edges below this weight are not followed during BFS. |
| `max_chunks` | `usize` | `8` | Total chunk budget in the assembled context. |
| `fts_share` | `f32` | `0.5` | Fraction of `max_chunks` reserved for FTS-only results (rest from KG). |
| `max_seeds` | `usize` | `5` | Maximum seed entities picked from the query (ranked by mention count). |

All parameters have TOML equivalents under `[agents.docs.kg]` — see [Configuration](#configuration) below.

---

## API

### Lifecycle

```rust
let store = IKGDocStore::open(agent_identity_dir)?;
```

Creates the `docs/` and `kg/` sub-directories and initialises the SQLite schema on first use.

### Base document store (mirrors `IDocStore`)

| Method | Behaviour |
|--------|-----------|
| `add_document(doc) → String` | Insert document; returns existing ID if same `content_hash` already stored. |
| `get_document(doc_id) → Document` | Fetch metadata + content. |
| `list_documents() → Vec<DocMetadata>` | All documents, newest first. |
| `delete_document(doc_id)` | Remove document, chunks, and content file. KG is **not** rebuilt automatically. |
| `chunk_document(doc_id, chunk_size) → Vec<Chunk>` | Markdown-aware split; does not index. |
| `index_chunks(chunks)` | Write chunks to FTS5 table (replaces existing chunks for same doc). |
| `search_by_text(query, top_k) → Vec<SearchResult>` | BM25 FTS search. |

### KG-specific helpers

| Method | Behaviour |
|--------|-----------|
| `all_chunks() → Vec<Chunk>` | Return every indexed chunk (used internally by `rebuild_kg`). |
| `get_chunks_by_ids(ids) → Vec<Chunk>` | Fetch specific chunks by ID in caller order. |

### KG Build

```rust
store.rebuild_kg()?;
// or with custom config and domain seeds:
store.rebuild_kg_with_config(&cfg, &[("rust", EntityKind::Term)])?;
```

Reads all indexed chunks, runs entity + relation extraction, writes `kg/`.  Safe to call multiple times — each call overwrites the previous KG.

**Domain seeds** are `(name, kind)` pairs that are matched case-insensitively and bypass the `min_entity_mentions` filter.

### KG Query

```rust
let result: KgSearchResult = store.search_with_kg(query, &cfg)?;
```

Returns a `KgSearchResult`:

```rust
pub struct KgSearchResult {
    pub context: String,          // assembled LLM context (KG summary + passages)
    pub used_kg: bool,            // false → pure FTS fallback was used
    pub seed_entities: Vec<String>,
}
```

---

## KG Build Pipeline

```
all_chunks()
    │
    ▼
Pass 1 — Entity extraction (per chunk)
    ├─ backtick terms     → Kind::Term
    ├─ double-quoted terms → Kind::Term
    ├─ CamelCase tokens    → Kind::System
    ├─ Title Case phrases  → Kind::Concept
    ├─ ACRONYMS            → Kind::Acronym
    └─ domain seeds        → provided Kind
    │
    ▼ accumulate mention_count (per-occurrence, not per-chunk)
    │
Filter: min_entity_mentions · len > 1 · not pure digits
    │
    ▼
Pass 2 — Relation extraction (per chunk)
    ├─ typed: "A uses B", "A implements B", "A extends B" …
    └─ fallback: co-occurrence → label "relates_to"
    │
    ▼ normalise weights to (0, 1]
    │
write_graph() → entities.json · relations.json · graph.json
```

---

## KG Query Pipeline

```
load_graph()
    │
    ▼ graph empty? → fts_only_result()
    │
Seed finding: match entity names against query_lower
    │ no seeds? → fts_only_result()
    │
Sort by mention_count, truncate to max_seeds
    │
BFS traversal (bfs_max_depth, edge_weight_threshold)
    │
Collect KG chunk pool + partial scores
    │
FTS search (ceil(max_chunks × fts_share) results)
    │
Merge + rank: score = 1.0 + kg_bonus + fts_bonus
    │
Truncate to max_chunks → fetch chunk texts
    │
KG summary (seed + top neighbours per seed)
    │
Context assembly:
    "## Knowledge Graph Context\n…\n## Relevant Passages\n[id | title]\n…"
```

---

## Fallback Behaviour

`search_with_kg` falls back to a pure FTS result (setting `used_kg = false`) in two cases:

1. **No graph exists** — `rebuild_kg` has not been called yet, or no chunks were indexed.
2. **No seeds matched** — none of the known entities appear in the query text.

The fallback format mirrors the KG path (`## Relevant Passages\n…`) so callers receive the same context shape regardless of path taken.

---

## Entity Extraction Details

### Mention counting

Entity `mention_count` is the **sum of per-occurrence counts** across all chunks, not just the number of chunks where it appears.  This means a term repeated 5 times in a single chunk accrues 5 mentions and can meet `min_entity_mentions = 2` from a single dense chunk.

### Filter rules (after counting)

| Rule | Rationale |
|------|-----------|
| `mention_count < min_entity_mentions` | Remove noise; seeds are exempt. |
| `len <= 1` | Single-character tokens are noise. |
| All ASCII digits | Numbers without context are not meaningful entities. |

### Sentence-start suppression (Title Case)

Title Case phrases at a sentence start (after `.`, `!`, `?`) are skipped to avoid treating normal sentence-opening capitalization as entity names.

---

## Relation Label Vocabulary

| Label | Trigger text |
|-------|-------------|
| `uses` | "uses" between A and B |
| `implements` | "implements" |
| `extends` | "extends" |
| `calls` | "calls" |
| `depends_on` | "depends on" |
| `requires` | "requires" |
| `defined_as` | "is a" / "is an" |
| `instance_of` | "refers to" / "instance of" |
| `relates_to` | fallback (plain co-occurrence) |

---

## Configuration

`KgConfig` parameters map to TOML fields under `[agents.docs.kg]`:

```toml
[agents.docs]
use_kg = true                # enable KG path (default: false)

[agents.docs.kg]
min_entity_mentions = 2
bfs_max_depth       = 2
edge_weight_threshold = 0.15
max_chunks          = 8
fts_share           = 0.50
max_seeds           = 5
```

All fields are optional and fall back to the defaults above.

---

## Cargo Features

| Feature | Enables |
|---------|---------|
| `ikgdocstore` | `IKGDocStore`, `KgGraph`, `KgConfig`, `KgSearchResult`, `docstore_core` |
| `plugin-docs-kg` | `subsystem-agents` + `ikgdocstore` — full docs agent with KG path |

```bash
cargo build  --features ikgdocstore
cargo test   --features ikgdocstore,subsystem-memory -- test_kg_docstore
```

---

## Testing

Integration tests live in `tests/test_kg_docstore.rs` (requires features `ikgdocstore,subsystem-memory`).

Key test cases:

| Test | Covers |
|------|--------|
| `open_creates_dirs` | Store opens and creates required directory tree. |
| `base_api_round_trip` | Add → chunk → index → search basic flow. |
| `dedup_by_hash` | Re-adding the same content returns existing ID. |
| `rebuild_kg_with_no_chunks_writes_empty_graph` | Empty corpus handled gracefully. |
| `rebuild_kg_extracts_camelcase_entities` | CamelCase tokens extracted and mention-counted. |
| `search_with_kg_falls_back_when_no_graph` | No error when graph absent. |
| `search_with_kg_uses_kg_when_entity_matched` | KG path taken when seed matches. |
| `search_with_kg_context_contains_kg_summary_section` | Output contains expected `## Knowledge Graph Context` section. |
| `idocstore_and_ikgdocstore_coexist` | Both stores work independently in the same agent directory. |

---

## Limitations (Phase 1)

- **No embeddings:** entity matching is lexical (substring); semantic synonyms are not linked.
- **No incremental KG updates:** `rebuild_kg` scans all chunks on every call; for large corpora, schedule it as an offline job.
- **No relation directionality at query time:** the BFS adjacency list is bidirectional regardless of the relation direction in the source.
- **No cross-agent graph queries:** each agent's KG is entirely isolated.

---

## Related Documentation

- [intelligent_doc_store.md](intelligent_doc_store.md) — `IDocStore` and shared `docstore_core` types
- [memory.md](memory.md) — Memory subsystem and agent identity directories
- [agents.md](agents.md) — Docs agent `use_kg` configuration
