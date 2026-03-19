# News KG Strategy: Summary & Evaluation

> Based on: "Talking to GDELT Through Knowledge Graphs" — Myers, Vargas, Aksoy, Joslyn, Wilson, Burke, Grimes (2025)
> https://arxiv.org/html/2503.07584v3

---

## 1. Paper Summary

### Core Question
How do different knowledge graph construction and retrieval strategies compare for answering questions over a real-world news corpus (GDELT)?

### Three KG Construction Approaches

| Approach | Method | Nodes / Edges | Source |
|---|---|---|---|
| **DKG** (Direct KG) | GDELT schema → graph directly | 3,469 / 18,052 | GDELT structured data |
| **LKG** (LlamaIndex KG) | LLM triple extraction (Mixtral-8x7B) | prescribed ontology | Raw article text |
| **GRKG** (GraphRAG KG) | Microsoft GraphRAG (Llama-3.1-8B) | free-form, no schema | Raw article text |

### Five QA Pipelines Tested
1. Direct Cypher-style queries on DKG (keyword → graph query)
2. G-Retriever (automated subgraph retrieval) on DKG
3. Classic vector store RAG on raw article chunks
4. G-Retriever on LLM-extracted LKG
5. GraphRAG's built-in QA on GRKG

### Case Study: Baltimore Bridge Collapse (March 26, 2024)
371 GDELT events, 2,047 mentions, 209 articles — filtered by `{Baltimore, bridge, collapse, ship}`.

### Results by Question Type

| Question Type | Best Performer | Why |
|---|---|---|
| Aggregate counts ("How many CNN articles?") | DKG direct query | Structured provenance, exact counts |
| Named entities ("What ship?") | LKG / GraphRAG | Extracted from article prose |
| Cross-document patterns | GraphRAG | Community detection across docs |
| Fine-grained excerpts | Vector store RAG | Proximity-based chunk retrieval |
| G-Retriever (both) | **Worst overall** | Subgraph extraction too noisy |

### Critical Failures of LLM-Generated KGs
- **Entity resolution:** "Container ship" ≠ "Container_ship" ≠ "DALI" ≠ "THE DALI"
- **Ontological drift:** LLMs hallucinate new edge types beyond the specification
- **Isolated nodes:** GraphRAG produced 435 orphan nodes out of 968 total
- **Inconsistency:** Same event described with different relation labels across documents

### Main Conclusion
> "The debate should not be of 'Either/Or', but rather integration between these two modalities."

DKG excels at structural/aggregate queries; vector RAG excels at fine-grained factual recall; LLM KGs capture summaries but lack consistency. Hybrid pipelines win.

---

## 2. Evaluation: Applying This to Araliya-Bot's News KG

### 2.1 Where the Current System Diverges from News Needs

The existing `IKGDocStore` was designed for **technical documentation** (software systems, code, APIs). Its entity kinds and relation labels reflect this:

```
EntityKind: Concept | System | Person | Term | Acronym
Relations:  uses | implements | extends | calls | depends_on | relates_to
```

For **ongoing news stories**, the semantic requirements are completely different:

| Need | Current System | Gap |
|---|---|---|
| Track people (politicians, officials) | `Person` exists | No role/affiliation |
| Track organizations | `System` (wrong kind) | No Org kind |
| Track locations | Missing | No Location kind |
| Track events with timestamps | Missing | No Event kind |
| Story continuity across days | Missing | No temporal edges |
| Causal chains (X led to Y) | Missing | No causal relations |
| Source provenance | `source_chunks` (chunk-level) | No per-claim sourcing |

### 2.2 Entity Extraction Quality for News Text

The current extractor uses **heuristic rules**, not LLM extraction:
- Backtick/quotes → `Term`
- CamelCase → `System`
- Title Case noun phrases → `Concept`
- ALL-CAPS → `Acronym`

**Problem:** News articles are written in prose. "The White House" is Title Case but it is a `Location`, not a `Concept`. "Francis Scott Key Bridge" is a `Concept` by the current rules but is actually a named infrastructure entity. "Dali" is the ship name but appears mid-sentence in non-Title-Case contexts and would be missed entirely.

The paper's DKG approach solves this by **using the GDELT schema directly** — GDELT already knows the actor type (country, organization, person), the event Goldstein scale, the geographic coordinates. The LLM approaches try to extract this from raw text and fail at entity resolution.

### 2.3 The Temporal Problem: Developing Stories

The paper focuses on a **single-day snapshot** (the bridge collapse). For monitoring *developing stories*, the entire temporal dimension is missing from the paper's approach — and from the current araliya-bot KG. This is the hardest and most important problem.

A developing story needs:
- **Story identity:** a persistent ID representing "the Baltimore Bridge collapse story"
- **Timeline edges:** `Article_A (T=day1) → PRECEDES → Article_B (T=day2)`
- **Delta tracking:** what changed between reporting cycles (new facts, corrections, new actors)
- **Story thread linking:** "bridge collapse" story connects to "supply chain disruption" story via shared entities (Port of Baltimore, shipping industry)
- **Salience decay:** entities that stop appearing in new articles should have reduced weight

---

## 3. Recommended KG Design for News Story Monitoring

### 3.1 New Entity Ontology

```rust
pub enum NewsEntityKind {
    Person,        // Named individual (politician, official, victim)
    Organization,  // Named org (NTSB, DOT, Maersk, CNN)
    Location,      // Named place (Baltimore, Chesapeake Bay, Maryland)
    Event,         // Discrete occurrence ("bridge collapse", "investigation launch")
    Topic,         // Thematic thread ("infrastructure safety", "supply chain")
    Vessel,        // Ships, vehicles (Dali container ship)
    Legislation,   // Bills, laws, regulations
    Source,        // News outlet (Reuters, AP, Baltimore Sun)
}
```

### 3.2 News-Specific Relation Types

```
// Temporal
"precedes"         Article/Event A occurred before B
"triggered"        Event A caused/triggered Event B
"updates"          Article B is an update to story thread A
"contradicts"      Claim A and Claim B conflict

// Actor-Event
"involved_in"      Person/Org participated in Event
"reported_by"      Event was reported by Source
"investigated_by"  Event is under investigation by Org
"affected_by"      Location/Person impacted by Event

// Story Threading
"part_of"          Event is part of Topic/story arc
"relates_to"       Cross-story connection (weak)
"escalated_to"     Story evolved into a larger story

// Provenance
"claimed_by"       Claim attributed to Person/Org
"sourced_from"     Fact sourced from publication
```

### 3.3 Story Thread Model

Instead of rebuilding the KG from scratch on every aggregation cycle (current behavior), use an **incremental append model** with an explicit story thread index.

```
StoryThread {
    id:             sha256(canonical_topic_phrase)[:16]
    canonical_name: "Baltimore Bridge Collapse"
    first_seen:     timestamp
    last_updated:   timestamp
    article_ids:    Vec<String>        // chronological
    seed_entities:  Vec<String>        // entity IDs anchoring this thread
    status:         Active | Dormant | Resolved
    parent_thread:  Option<String>     // e.g. "US Infrastructure" thread
}
```

**Thread detection:** when a new article arrives, check if its dominant entities overlap with an existing thread's seed entities (Jaccard similarity > 0.3). If yes, attach to existing thread and update `last_updated`. If no significant overlap, create a new thread. This replaces the blunt full-KG-rebuild that currently happens after every aggregation batch.

### 3.4 Hybrid Pipeline (Paper's Main Recommendation Applied)

The paper's conclusion maps cleanly onto a three-layer pipeline for news:

```
Layer 1: STRUCTURED  (DKG-equivalent)
  Source:   GDELT feed or structured event records
  Provides: actor types, event codes, geo, Goldstein scale, article counts
  Best for: aggregate queries, temporal ordering, actor networks

Layer 2: KG  (LLM-assisted, ontology-guided)
  Source:   Article summaries → guided triple extraction
  Uses:     Prescribed NewsEntityKind + relation types above
  Includes: Entity normalization / coreference resolution pass
  Best for: causal chains, quote attribution, story threading

Layer 3: VECTOR  (current chunk FTS — keep as-is)
  Source:   Raw article text chunks, BM25 + FTS
  Best for: Fine-grained factual recall, quotes, specific numbers
```

**Query routing:**

| Question | Layer |
|---|---|
| "Who was involved in X?" | Layer 2 KG (entity-event relations) |
| "How many articles covered X?" | Layer 1 structured |
| "What exactly did the NTSB say?" | Layer 3 vector |
| "How did the story evolve?" | Layer 1 + 2 combined (temporal traversal) |

---

## 4. Concrete Implementation Changes for Araliya-Bot

### Priority 1 — News-specific entity extraction
Extend `extract_entities_from_text` with a **news mode** flag. In news mode:
- Suppress CamelCase → `System` heuristic (fires falsely on prose proper nouns)
- Add LLM-based NER using a small extraction prompt with `NewsEntityKind` as the schema
- Extract **per article summary** (not per chunk) — summaries are already LLM-generated and cleaner

### Priority 2 — Incremental KG update (not full rebuild)
Replace `rebuild_kg_with_config` in the news aggregator with an `append_to_kg` pass:
1. Extract entities + relations from the new article only
2. Merge into existing graph: entity dedup by normalized name + kind
3. Update `mention_count`, `source_chunks`, edge weights incrementally
4. Reserve full-rebuild for explicit `reset` commands or major corpus shifts

This is the paper's implicit recommendation: DKG stays fresh because GDELT is a structured stream; LLM KGs require expensive full rebuilds and that cost compounds with corpus size.

### Priority 3 — Temporal edges
Add `first_seen_at: u64` and `last_seen_at: u64` to `Entity`. Add `published_at: Option<DateTime>` to `Document`. This enables:
- BFS traversal filtered by time window ("entities active in the last 7 days")
- Story arc detection: same entity cluster reappearing across multiple time windows = developing story

### Priority 4 — Story thread index
Add `story_threads.json` alongside `entities.json` in the KG directory. The `news_aggregator` agent writes and updates this on each aggregation cycle. The `newsroom` agent reads it to compile cross-article summaries ordered by story arc rather than by publication date, enabling output like "3 updates to the bridge collapse story, 1 new story emerging."

### Priority 5 — Entity resolution pass
The paper's most painful failure was entity resolution (DALI vs THE DALI). After extraction, run a normalization pass before ID generation:
- Strip leading articles (the, a, an)
- Lowercase + strip punctuation (already done for the `id` field)
- Alias map: store aliases on the entity, merge mention counts
- For `Person` entities: last-name dedup within a story thread scope
- For `Organization` entities: acronym expansion lookup (NTSB → National Transportation Safety Board)

---

## 5. What NOT to Copy from the Paper

| Paper approach | Why to avoid |
|---|---|
| G-Retriever for subgraph retrieval | Worst performer in the paper; current BFS is simpler and more reliable |
| GraphRAG community detection | Expensive; 45% orphan node rate; overkill for article-scale corpus |
| LLM triple extraction per chunk | Per-chunk LLM calls are expensive; extract per-summary instead |
| Cosine similarity as sole eval metric | Useful for paper's controlled evaluation; not needed in production |

---

## 6. Summary Table

| Capability | Current Araliya-Bot | Paper Finding | Recommended Change |
|---|---|---|---|
| Entity types | Tech-oriented (System, Term) | News needs Person, Org, Location, Event | Add `NewsEntityKind` enum |
| Extraction method | Regex / heuristics | Heuristics miss news entities; LLM needs ontology anchoring | LLM extraction with prescribed schema per article summary |
| KG rebuild | Full rebuild each cycle | Expensive; causes entity count instability | Incremental append + alias dedup |
| Temporal tracking | None | Critical for developing stories | `first_seen_at`, `last_seen_at` per entity; temporal edges |
| Story threading | None | Core differentiator vs. one-shot QA | `StoryThread` index, cross-story entity overlap detection |
| Query routing | KG + FTS hybrid | Integration beats either/or | Add structured GDELT layer for aggregate queries |
| Entity resolution | sha256(normalized name) | Major failure mode in LLM KGs | Normalize + alias map before ID generation |
| Source provenance | Chunk-level only | Needed for cross-source fact checking | Per-claim `sourced_from` relation |

---

## 7. Verdict

The paper validates araliya-bot's hybrid KG+FTS design as the right architectural direction. The gaps are almost entirely in the **news-domain ontology** and **temporal/story-threading logic** — both tractable additions that do not require architectural changes, only extension of the existing `IKGDocStore` and `news_aggregator` agent.

The single highest-leverage change is **Priority 4** (story thread index): it turns the aggregator from a static document store into a genuine story tracker, and it directly enables the "connecting developing stories to larger past stories" use case by making cross-thread entity overlap queryable.
