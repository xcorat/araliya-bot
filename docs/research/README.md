# Research Documentation: Large Rust Project Organization

This directory contains comprehensive research on how large Rust projects organize their code, with specific focus on async systems and message-bus architectures like Araliya-Bot.

---

## Documents

### 1. [large-project-organization-patterns.md](./large-project-organization-patterns.md)

**Scope:** High-level architectural patterns across major Rust projects

**Contents:**
- Key findings from Tokio, Tonic, Tower, Bevy, Embassy, Quinn, and Redis
- Workspace architecture patterns (monolithic vs multi-crate)
- Compile-time vs runtime modularity trade-offs
- Subsystem boundary design principles
- Plugin system approaches (compile-time vs runtime loading)
- Public API design patterns
- Analysis of Araliya-Bot's current architecture
- Recommendations for maintenance and future growth

**Read this first** for a high-level overview of what large projects do and how Araliya-Bot compares.

---

### 2. [workspace-patterns-examples.md](./workspace-patterns-examples.md)

**Scope:** Detailed workspace organizational examples from 6 major projects

**Contents:**
- Tokio's single-crate, feature-gated model
- Tonic's multi-crate workspace with trait-based plugins
- Quinn's protocol/transport separation
- Bevy's aggressive feature preset management
- Embassy's trait-based HAL abstraction
- Araliya-Bot's current single-crate with feature-gated subsystems
- Side-by-side comparison table
- Decision matrix: which pattern to choose in different scenarios

**Read this** for concrete examples of how successful projects structure their code and Cargo workspaces.

---

### 3. [async-subsystem-design.md](./async-subsystem-design.md)

**Scope:** Deep technical patterns for async subsystem organization and supervision

**Contents:**
- Supervisor/dispatcher hub pattern (used by Araliya-Bot)
- Peer-to-peer mesh pattern (alternative)
- Layered/hierarchical pattern (alternative)
- Supervision models (panic recovery, graceful degradation, checkpointing)
- Message passing strategies (request-response, fire-and-forget, pub-sub)
- Subsystem lifecycle management (initialization, shutdown)
- Error handling patterns (typed errors, error context, structured errors)
- Resilience patterns (exponential backoff, circuit breaker)
- Araliya-Bot's specific improvements and recommendations

**Read this** when designing subsystems or when you need to understand why Araliya-Bot made specific architectural choices.

---

## Key Takeaways

### For Araliya-Bot

**Current Architecture Assessment:** ✅ **Excellent fit**

Araliya-Bot correctly implements industry-standard patterns:

1. **Single monolithic crate** with feature-gated subsystems
   - Optimal for team size and codebase maturity
   - Simpler than multi-crate workspace
   - Allows shared infrastructure (config, errors)

2. **Supervisor/dispatcher hub** with non-blocking routing
   - Proven pattern (Erlang OTP, Akka)
   - Enables star topology (no dependency cycles)
   - Centralized control (logging, permissions)

3. **Trait-based subsystem boundaries**
   - `BusHandler` (subsystems), `Agent` (plugins), `Component` (long-running tasks)
   - Loose coupling; easy to test

4. **Feature-gated compile-time modularity**
   - Binary size optimized
   - No runtime dispatch overhead
   - Dependency consistency enforced by compiler

5. **Graceful shutdown** with CancellationToken
   - Standard Tokio idiom
   - All subsystems receive signal simultaneously

### When to Refactor

**Do NOT extract to multi-crate workspace unless:**
- Team grows to 5+ engineers per subsystem
- Subsystems stabilize (boundaries won't change)
- Crate is published separately
- Compilation time is a bottleneck (measure first)

**Current single-crate architecture will scale to ~100k lines of Rust code without issues.**

### What to Improve

1. **Resilience:** Add retry logic and circuit breakers for external API calls
2. **Observability:** Expose detailed health metrics per subsystem
3. **Testing:** Add integration tests for feature combinations and shutdown sequences
4. **Documentation:** Document plugin registry, feature matrix, initialization order

---

## Patterns Reference

### Workspace Models

| Model | Example | Best For | Araliya-Bot |
|-------|---------|----------|-------------|
| Single crate + features | Tokio, Bevy | Medium teams, shifting boundaries | ✅ Current approach |
| Multi-crate workspace | Tonic, Quinn | Large teams, stable subsystems | 🔮 Future option |
| Hybrid | Bevy (extended) | Gradual extraction | 🔮 Future path |

### Supervision Models

| Model | Recovery | State Loss | Best For |
|-------|----------|-----------|----------|
| Panic recovery | Restart | All in-flight | Ephemeral state, retryable |
| Graceful degradation | Manual | None | Data consistency critical |
| Checkpointed restart | Automatic | Since checkpoint | Stateful with monitoring |

**Araliya-Bot:** Uses panic recovery (correct for agents; consider graceful degradation for tools/news)

### Message Patterns

| Pattern | Latency | Coupling | Best For |
|---------|---------|----------|----------|
| Request-response | Medium | Tight (RPC-like) | Control flow, error handling |
| Fire-and-forget | Low | Loose | Events, logging, non-critical |
| Pub-subscribe | Medium | Loose | Multi-subscriber events |

**Araliya-Bot:** Uses request-response; fire-and-forget supported but rarely used

---

## Architecture Checklist

**For new subsystems or major refactors, ensure:**

- [ ] Subsystem has unique `BusHandler::prefix()`
- [ ] All external dependencies declared in Cargo features
- [ ] Feature gate: `subsystem-{name}` or `plugin-{name}`
- [ ] Handler registered in `main.rs` behind feature gate
- [ ] `{prefix}/status` route implemented
- [ ] Error types contribute to main `AppError` enum
- [ ] Unit tests in `#[cfg(test)]` block
- [ ] Respects `shutdown: CancellationToken` in all long-running tasks
- [ ] Documentation in architecture guide
- [ ] Example config in `config/default.toml`

---

## Further Reading

### External References

- **Tokio:** https://github.com/tokio-rs/tokio — Async runtime reference
- **Tonic:** https://github.com/hyperium/tonic — Modular workspace pattern
- **Quinn:** https://github.com/quinn-rs/quinn — Protocol/transport separation
- **Bevy:** https://github.com/bevyengine/bevy — Feature-gated complexity
- **Embassy:** https://github.com/embassy-rs/embassy — Trait-based HAL
- **Erlang OTP:** https://www.erlang.org/doc/design_principles/des_princ.html — Supervisor behavior (actor model inspiration)
- **Akka:** https://akka.io/docs/ — Modern actor model (JVM equivalent)

### Related Araliya-Bot Docs

- `docs/architecture/overview.md` — Current system architecture
- `docs/architecture/subsystems/agents.md` — Agents subsystem (largest, most complex)
- `docs/architecture/standards/plugin-interfaces.md` — How to add agents/providers
- `docs/architecture/standards/bus-protocol.md` — JSON-RPC 2.0 protocol spec
- `CLAUDE.md` — Development guide (build, test, logging)

---

## How to Use This Research

### Use Case 1: Understanding Araliya-Bot's Architecture

1. Read [large-project-organization-patterns.md](./large-project-organization-patterns.md) → "Araliya-Bot: Current Analysis" section
2. Read [async-subsystem-design.md](./async-subsystem-design.md) → "Araliya-Bot Analysis" section
3. Reference: `docs/architecture/overview.md` for detailed subsystem descriptions

### Use Case 2: Adding a New Subsystem

1. Check [workspace-patterns-examples.md](./workspace-patterns-examples.md) → "Araliya-Bot" section for structure
2. Reference: "Architecture Checklist" above
3. Look at existing subsystem (e.g., `tools/`, `cron/`) as template
4. Implement `BusHandler` trait following `subsystems/dispatch.rs` pattern

### Use Case 3: Refactoring (e.g., Agent Module Too Large)

1. Read [workspace-patterns-examples.md](./workspace-patterns-examples.md) → "Comparison Table"
2. Check `async-subsystem-design.md` → "Subsystem Lifecycle Management"
3. Consider: Split into agent families (chat/, docs/, news/) **without changing workspace structure**
4. Measure: Does refactoring improve compilation time? (benchmark first)

### Use Case 4: Considering Multi-Crate Extraction

1. Read [large-project-organization-patterns.md](./large-project-organization-patterns.md) → "When to extract crates vs keep in one crate"
2. Check [workspace-patterns-examples.md](./workspace-patterns-examples.md) → "Decision Matrix"
3. Only proceed if **all criteria are met:**
   - Subsystem is stable (no API changes expected)
   - Another project wants to reuse it
   - Team is large enough to own it independently
   - Compilation time is a measured bottleneck

### Use Case 5: Improving Resilience

1. Read [async-subsystem-design.md](./async-subsystem-design.md) → "Error Handling & Resilience"
2. Check "Recommendations" section
3. Implement retry logic for external API calls (LLM, tools)
4. Add circuit breaker for slow downstream services

---

## Document Summary

| Document | Pages | Audience | Time |
|----------|-------|----------|------|
| large-project-organization-patterns.md | 8 | Architects, team leads | 30 min |
| workspace-patterns-examples.md | 12 | Developers, refactoring work | 45 min |
| async-subsystem-design.md | 10 | Subsystem owners, senior devs | 40 min |

**Total reading time:** ~2 hours for full understanding

**Minimum viable reading:** 30 min
- Read section 1-2 of large-project-organization-patterns.md
- Skim Araliya-Bot section

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-03-19 | Initial research documentation |

---

## Contributing to Research Docs

When adding new research or patterns:

1. Keep documents focused (one pattern per document)
2. Include concrete examples from real projects
3. Add trade-offs section (not just benefits)
4. Link to external references
5. Relate back to Araliya-Bot's choices
6. Update this README with new doc

---

**Research maintained by:** Claude Code
**Last updated:** 2026-03-19
**Review date:** 2026-06-19 (quarterly)
