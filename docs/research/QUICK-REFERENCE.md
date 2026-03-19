# Quick Reference: Organization Patterns

**One-page cheat sheet for Rust project organization patterns.**

---

## Pattern Decision Tree

```
Do you have subsystems that need to:

Q1: Communicate with each other?
├─ YES → Q2
└─ NO → Probably don't need organization patterns

Q2: Change their boundaries frequently?
├─ YES → Single monolithic crate with feature gates (Tokio model)
└─ NO → Q3

Q3: Are they stable and published separately?
├─ YES → Multi-crate workspace (Tonic model)
└─ NO → Single crate with feature gates

Q4: Do you have >100 subsystems or complex protocols?
├─ YES → Consider layered crates (Quinn model)
└─ NO → Stick with single-crate model
```

---

## Pattern Comparison (Quick)

| Aspect | Tokio-style | Tonic-style | Quinn-style |
|--------|-----------|-----------|-----------|
| **Crates** | 1 (+ utils) | 5+ | 3 (proto/transport/wrapper) |
| **Setup time** | Fast | Medium | Complex |
| **Refactoring** | Easy | Hard | Hard |
| **Binary size** | Tiny | Small | Tiny |
| **Team overhead** | Low | High | High |
| **When to use** | Medium-sized teams | Large teams | Protocol emphasis |
| **Araliya-Bot** | ✅ | 🔮 | ❌ |

---

## Supervision Models (Quick)

| Model | Mechanism | State Loss | Error Handling | Recovery |
|-------|-----------|-----------|---|----------|
| **Panic** | Component fails → supervisor restarts | All | Automatic restart | Retry all |
| **Graceful** | Component fails → marked offline | None | Manual intervention | Manual reset |
| **Checkpoint** | Periodic save → resume from checkpoint | Since checkpoint | Automatic resume | Resume from disk |

**Araliya-Bot uses:** Panic recovery (correct for agents; consider graceful for tools)

---

## Bus Patterns (Quick)

| Pattern | Latency | Blocking | Error | Coupling | Example |
|---------|---------|----------|-------|----------|---------|
| **RPC** | Medium | Caller | Yes | Tight | `agents/chat/handle` |
| **Fire-forget** | Low | No | No | Loose | `cron/tick` |
| **Pub-sub** | Medium | No | No | Loose | Event topic |

**Araliya-Bot uses:** RPC (request-response) primarily

---

## Feature Hierarchy (Araliya-Bot)

```
Presets:
  minimal = [core + 1 plugin]
  default = [core + several plugins]
  full = [default + all plugins]

Subsystem gates:
  subsystem-agents ← requires subsystem-memory
  subsystem-llm
  subsystem-comms
  subsystem-tools

Plugin gates:
  plugin-basic-chat ← requires subsystem-llm
  plugin-chat ← requires subsystem-llm + subsystem-memory
  plugin-docs ← requires idocstore
  ...

Channel gates:
  channel-pty ← requires subsystem-comms
  channel-axum ← requires subsystem-comms + axum
  ...

UI gates:
  ui-svui ← requires subsystem-ui
  ui-gpui ← requires gpui deps
  ...

Best practices:
  ✅ Document each gate's dependencies
  ✅ Test key combinations (minimal, default, full)
  ✅ Keep number of gates < 30 (explodes in complexity)
```

---

## Code Organization Checklist

**For any new subsystem:**

```
□ Define error variants in AppError enum
□ Create BusHandler implementation with unique prefix
□ Implement {prefix}/status route
□ Add feature gate (subsystem-NAME or plugin-NAME)
□ Register in main.rs behind feature gate
□ All long-running tasks respect shutdown token
□ Unit tests in #[cfg(test)] blocks
□ Document in architecture/subsystems/{name}.md
□ Add example config in config/default.toml
□ Update PLUGIN_REGISTRY.md (if agent plugin)
```

---

## When to Extract to Multi-Crate

**Only if ALL are true:**

- [ ] Subsystem is stable (no API changes > 3 months)
- [ ] Another project wants to reuse it
- [ ] Team has 3+ engineers dedicated to it
- [ ] Compilation time is measured bottleneck
- [ ] You've measured benefit of extraction

**For Araliya-Bot:** Memory subsystem is only reuse candidate; not yet ready (too coupled to agent auth).

---

## Common Mistakes

❌ **Adding subsystems without feature gates**
- Forces all users to compile them
- Bloats binary for minimal setups
- Use: `subsystem-NAME` feature gate

❌ **Subsystems directly calling each other**
- Creates dependency cycles
- Breaks error isolation
- Use: Bus messages only

❌ **Global state (lazy_static, once_cell)**
- Hides dependencies
- Makes testing hard
- Use: Capability passing (Arc<State>)

❌ **Unguarded async operations**
- Subsystem spawn tasks but don't respect shutdown token
- Causes hangs on shutdown
- Use: `tokio::select! { _ = shutdown.cancelled() }`

❌ **Unbounded queues**
- Memory pressure under load
- No backpressure
- Use: Bounded channels or request timeout

---

## Araliya-Bot Score Card

| Aspect | Score | Notes |
|--------|-------|-------|
| **Workspace organization** | 9/10 | Good monolithic + features; consider modularizing agents/ |
| **Subsystem boundaries** | 9/10 | Trait-based; clean; no cycles |
| **Error handling** | 8/10 | Good typed errors; could add retry logic |
| **Testing** | 6/10 | Unit tests exist; missing integration tests for features |
| **Documentation** | 7/10 | Good architecture docs; missing plugin registry |
| **Resilience** | 6/10 | No retry logic; no circuit breaker; manual recovery only |
| **Observability** | 5/10 | Basic status; missing detailed metrics per subsystem |

**Overall:** 7.5/10 — Well-architected; good for growth; improvements in resilience and testing

---

## Quick Fix Ideas

**Low effort, high impact:**

1. **Add feature matrix test in CI:**
   ```bash
   cargo build --no-default-features --features minimal
   cargo build --no-default-features --features default
   cargo build --all-features
   ```
   *Time: 30 min; benefit: catch feature breakage early*

2. **Add retry logic to external API calls:**
   ```rust
   with_exponential_backoff(
       || call_llm_api(...),
       max_retries: 3
   ).await
   ```
   *Time: 2 hours; benefit: resilience vs transient failures*

3. **Document plugin registry:**
   ```markdown
   # Agent Registry
   | ID | Feature | Runtime Class |
   |----|---------|---|
   | echo | plugin-echo | RequestResponse |
   ```
   *Time: 1 hour; benefit: clarity for new developers*

4. **Add shutdown integration test:**
   ```rust
   #[tokio::test]
   async fn test_graceful_shutdown() {
       // Start system, signal shutdown, assert cleanup
   }
   ```
   *Time: 1.5 hours; benefit: catch resource leaks*

---

## Reading Order

**5 minutes:** This document + "Pattern Decision Tree"

**30 minutes:**
1. Large-project-organization-patterns.md (first 2 sections)
2. Araliya-Bot analysis section

**2 hours:** All three research documents

**Deep dive:** Async-subsystem-design.md when designing resilience

---

## Links

- **Overview:** `docs/architecture/overview.md`
- **Agents:** `docs/architecture/subsystems/agents.md`
- **Bus protocol:** `docs/architecture/standards/bus-protocol.md`
- **Building:** `CLAUDE.md` (build commands, testing)

---

**Last updated:** 2026-03-19
