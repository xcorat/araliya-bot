# Async Subsystem Design: Patterns & Trade-offs

**Deep dive into how successful async systems organize subsystems and manage boundaries.**

---

## Table of Contents

1. [Fundamental Patterns](#fundamental-patterns)
2. [Supervision Models](#supervision-models)
3. [Message Passing Strategies](#message-passing-strategies)
4. [Subsystem Lifecycle Management](#subsystem-lifecycle-management)
5. [Error Handling & Resilience](#error-handling-amp-resilience)
6. [Araliya-Bot Analysis](#araliya-bot-analysis)

---

## Fundamental Patterns

### Pattern 1: Supervisor/Dispatcher Hub

**Used by:** Araliya-Bot, Akka (JVM), Erlang OTP

**Model:**
```
        ┌─────────────────────────────┐
        │  Supervisor (Non-blocking)  │
        │  - Router                   │
        │  - Dispatcher               │
        │  - No state                 │
        └──────────────────┬──────────┘
                           │
         ┌─────────────────┼──────────────┬──────────────┐
         │                 │              │              │
         ▼                 ▼              ▼              ▼
    ┌────────┐        ┌────────┐    ┌────────┐    ┌────────┐
    │ Agents │        │ Memory │    │  LLM   │    │ Tools  │
    │  Sub   │        │ System │    │  Sub   │    │  Sub   │
    └────────┘        └────────┘    └────────┘    └────────┘
```

**Characteristics:**
- **Single router** dispatches all messages
- Router is **non-blocking:** forwards `reply_tx` and returns immediately
- **Star topology:** subsystems cannot directly contact each other
- Message format is **language-agnostic** (JSON-RPC 2.0)

**Advantages:**
- ✅ Centralized control (logging, permission checking)
- ✅ No dependency cycles possible (star forces DAG)
- ✅ Supervision points are explicit
- ✅ Can upgrade to multi-process without changing protocol

**Disadvantages:**
- ❌ All communication goes through supervisor (scalability limit ~100k msg/sec per core)
- ❌ Fault in supervisor affects all subsystems

**When to use:**
- Systems < 1M msg/sec throughput
- Clear, stable subsystem boundaries
- Centralized control important
- Team needs to understand architecture at a glance

**Araliya-Bot:** Uses this pattern ✅

---

### Pattern 2: Peer-to-Peer Mesh

**Used by:** CQRS/Event Sourcing systems, some microservice architectures

**Model:**
```
    ┌────────┐    ┌────────┐
    │ Agents │◄──►│ Memory │
    └────────┘    └────────┘
         ▲             ▲
         │ ◄─── ────► │
    ┌────┴────┐    ┌──┴─────┐
    │   LLM   │    │  Tools  │
    └─────────┘    └─────────┘
```

**Characteristics:**
- Subsystems directly call each other
- No central router
- Each subsystem has outbound queues to N other subsystems

**Advantages:**
- ✅ Direct communication (lower latency)
- ✅ Scales to high throughput (parallel queues)

**Disadvantages:**
- ❌ Complex dependency management (can create cycles)
- ❌ Harder to reason about control flow
- ❌ Difficult to test in isolation
- ❌ Permission checking must be per-subsystem

**When to use:**
- Very high throughput (millions msg/sec)
- Subsystems are peer-level
- Team is experienced with distributed systems

**Araliya-Bot consideration:** Not suitable (adds unnecessary complexity for medium throughput)

---

### Pattern 3: Layered/Hierarchical

**Used by:** Desktop GUIs (Qt, GTK), some embedded systems

**Model:**
```
User Interface Layer
      │
      ▼
Business Logic Layer
      │
      ▼
Data Storage Layer
      │
      ▼
Hardware Abstraction Layer
```

**Characteristics:**
- Strict layer ordering
- Upper layer depends on lower layer only
- Each layer provides a stable interface to upper layers

**Advantages:**
- ✅ Clear dependency graph (no cycles)
- ✅ Easy to test (mock lower layers)
- ✅ Natural separation of concerns

**Disadvantages:**
- ❌ Lower layers can't call upper layers (requires callbacks)
- ❌ Bypass for performance reasons leads to complexity

**When to use:**
- Clear input→process→output flow
- Subsystems have natural hierarchy
- Strong coupling between layers acceptable

**Araliya-Bot consideration:** Comms → Agents → LLM is somewhat layered, but not strictly (agents can call tools directly). Current star topology is more flexible.

---

## Supervision Models

### Model 1: Panic Recovery (Restart)

**Used by:** Erlang OTP, Tokio (with workarounds), Araliya-Bot

**Mechanism:**
```rust
// Supervisor receives error from subsystem task
loop {
    match handle.await {
        Ok(_) => {}  // Clean exit
        Err(e) => {  // Task panicked or error
            error!("subsystem failed: {e}");
            // Restart subsystem
            handle = spawn_subsystem();
        }
    }
}
```

**Characteristics:**
- Subsystem panics or returns error
- Supervisor restarts subsystem
- State in subsystem is lost

**Advantages:**
- ✅ Simple to implement
- ✅ Guaranteed cleanup (drop all resources)
- ✅ Can apply backoff strategies

**Disadvantages:**
- ❌ In-flight requests lost
- ❌ No graceful degradation
- ❌ May mask programming bugs

**Code example (Araliya-Bot pattern):**
```rust
pub fn spawn_components(
    components: Vec<Box<dyn Component>>,
    shutdown: CancellationToken,
) -> SubsystemHandle {
    let handle = tokio::spawn(async move {
        let mut set: JoinSet<Result<(), AppError>> = JoinSet::new();
        for component in components {
            set.spawn(component.run(shutdown.clone()));
        }

        while let Some(res) = set.join_next().await {
            match res {
                Ok(Ok(())) => {}     // Clean exit
                Ok(Err(e)) => {
                    error!("component error: {e}");
                    shutdown.cancel();  // Signal other components
                    return Err(e);
                }
                Err(e) => {
                    error!("component panicked: {e}");
                    shutdown.cancel();
                    return Err(...);
                }
            }
        }
        Ok(())
    });
    SubsystemHandle { inner: handle }
}
```

**When to use:**
- Subsystem state is ephemeral
- Clients can retry failed requests
- Availability > consistency

---

### Model 2: Graceful Degradation

**Used by:** Kubernetes (pod-level), Redis (replica failover)

**Mechanism:**
```rust
// Supervisor doesn't restart failed subsystem
// Instead, marks it as "offline" and routes requests to alternatives

pub enum SubsystemState {
    Running,
    Degraded(String),  // error message
    Offline,
}

// Request handler:
if state.is_online() {
    route_to_subsystem(request)
} else {
    // Fallback or return error
    reply_tx.send(Err("subsystem offline".into()))
}
```

**Characteristics:**
- Subsystem fails; supervisor marks as offline
- Requests to offline subsystem get error or fallback
- No restart; manual intervention needed

**Advantages:**
- ✅ Prevents cascading failures
- ✅ Clear error signals to clients
- ✅ No risk of restart loops

**Disadvantages:**
- ❌ Requires monitoring/alerting for recovery
- ❌ Service stays degraded until human intervenes
- ❌ Clients must handle "service offline" error

**When to use:**
- Data consistency critical (can't lose state)
- Human monitoring in place
- Better to be unavailable than incorrect

**Araliya-Bot consideration:** Agents subsystem is critical; if it fails, bot is useless. Current panic recovery is correct. Memory subsystem: graceful degradation might be useful (store to memory fails, but agent still works). Not implemented yet.

---

### Model 3: Checkpointed Restart

**Used by:** Kafka brokers, some distributed DBs, Flink

**Mechanism:**
```rust
// Subsystem periodically saves state checkpoint
// On restart, load from checkpoint and resume

pub struct Subsystem {
    state: Arc<Mutex<SubsystemState>>,
    checkpoint_dir: PathBuf,
}

impl Subsystem {
    async fn checkpoint(&self) {
        let state = self.state.lock().unwrap();
        save_to_disk(&state, &self.checkpoint_dir).await;
    }

    async fn recover() -> Result<Self> {
        let state = load_from_disk(&checkpoint_dir).await?;
        Ok(Subsystem {
            state: Arc::new(Mutex::new(state)),
            checkpoint_dir,
        })
    }
}
```

**Characteristics:**
- Regular checkpoint interval (every N seconds)
- On restart, resume from last checkpoint
- Some messages may be reprocessed

**Advantages:**
- ✅ Recoverable state
- ✅ Can resume in-flight operations
- ✅ Better than complete restart

**Disadvantages:**
- ❌ Checkpoint I/O overhead
- ❌ Window of lost updates (since last checkpoint)
- ❌ Complex implementation

**When to use:**
- State is expensive to recompute
- Some message replay acceptable
- Availability and partial consistency needed

**Araliya-Bot consideration:** Memory subsystem could use checkpointing (session state is valuable). Not implemented. Would be useful for multi-node setups.

---

## Message Passing Strategies

### Strategy 1: Request-Response (RPC-style)

**Pattern:**
```rust
pub struct Request {
    method: String,
    payload: BusPayload,
    reply_tx: oneshot::Sender<BusResult>,
}

// Synchronous from caller's perspective (awaited)
let result = bus.call("agents/echo/handle", payload).await?;
```

**Characteristics:**
- Caller waits for response
- Request/response are paired
- Errors are explicit (Result<T, E>)

**Advantages:**
- ✅ Familiar RPC model
- ✅ Easy error handling
- ✅ Synchronous flow (like normal function calls)

**Disadvantages:**
- ❌ Caller blocked (can't do other work)
- ❌ Timeout management complex
- ❌ Can create cascading timeouts (A waits for B waits for C)

**Used in:** Araliya-Bot (bus calls), most RPC systems

---

### Strategy 2: Fire-and-Forget

**Pattern:**
```rust
pub struct Notification {
    method: String,
    payload: BusPayload,
    // No reply_tx; caller doesn't wait
}

// Non-blocking from caller's perspective
bus.notify("cron/scheduler/tick", payload);  // Returns immediately
```

**Characteristics:**
- Caller doesn't wait for response
- No error handling (fire-and-forget)
- Lowest latency

**Advantages:**
- ✅ Non-blocking; caller free to do other work
- ✅ Lowest latency
- ✅ No timeout management

**Disadvantages:**
- ❌ No error feedback
- ❌ Delivery not guaranteed
- ❌ Hard to debug (lost messages)

**Used in:** Event systems, logging

**Araliya-Bot consideration:** Notifications are supported via `handle_notification()` in `BusHandler`. Rarely used; most operations need responses.

---

### Strategy 3: Publish-Subscribe

**Pattern:**
```rust
pub struct Topic {
    subscribers: Vec<mpsc::Sender<Event>>,
}

// Publisher
topic.publish(Event::SessionCreated { session_id });

// Subscriber
while let Some(event) = rx.recv().await {
    match event {
        Event::SessionCreated { session_id } => { /* ... */ }
    }
}
```

**Characteristics:**
- One publisher, many subscribers
- Subscribers register themselves
- Event-driven model

**Advantages:**
- ✅ Loose coupling (publisher doesn't know subscribers)
- ✅ Natural for multi-subscriber scenarios
- ✅ Scales to many subscribers

**Disadvantages:**
- ❌ No feedback to publisher (did anyone receive?)
- ❌ Ordering guarantees complex
- ❌ Requires explicit registration

**Used in:** Event sourcing, UI frameworks, message brokers

**Araliya-Bot consideration:** Not currently used. Could be useful for UI events (session started, message received). Would require additional subsystem for event broker.

---

## Subsystem Lifecycle Management

### Initialization Order (Critical)

**Araliya-Bot pattern (from main.rs):**
```rust
// 1. Load config
let config = config::load()?;

// 2. Setup identity
let identity = identity::setup(&config)?;

// 3. Setup logging
logger::init(&config.log_level)?;

// 4. Start supervisor bus
let bus = SupervisorBus::new();

// 5. Spawn subsystems
#[cfg(feature = "subsystem-memory")]
let memory = MemorySystem::new(&config).await?;

#[cfg(feature = "subsystem-agents")]
let agents = AgentsSubsystem::new(&config, bus.clone(), memory.clone())?;

#[cfg(feature = "subsystem-llm")]
let llm = LlmSubsystem::new(&config)?;

// 6. Register handlers
bus.register(Arc::new(agents))?;
bus.register(Arc::new(llm))?;

// 7. Start supervisor loop
supervisor::run(bus, shutdown_token).await?;

// 8. Run comms (blocks until shutdown)
comms::run(bus.clone()).await?;
```

**Key principles:**
1. **Low-level first:** Core infrastructure (logger, identity) before subsystems
2. **Dependencies first:** Memory before agents (agents depend on memory)
3. **Handlers last:** Register after all subsystems created
4. **Supervisor last:** Start routing after all handlers registered

**Why this matters:**
- ❌ Wrong order: Memory tries to start before logger initialized → crash
- ✅ Right order: Each subsystem has what it needs before it starts

---

### Shutdown Sequence (Graceful)

**Pattern (Araliya-Bot uses `CancellationToken`):**
```rust
// Main process
let shutdown = CancellationToken::new();

// Spawn subsystems
let agents_handle = spawn(agents_subsystem(shutdown.clone()).run());
let memory_handle = spawn(memory_subsystem(shutdown.clone()).run());

// Wait for Ctrl-C
tokio::signal::ctrl_c().await?;

// Signal shutdown
shutdown.cancel();

// Wait for subsystems to finish
agents_handle.await?;
memory_handle.await?;

// Cleanup
logger::shutdown();
```

**Component level (from runtime.rs):**
```rust
pub trait Component: Send + 'static {
    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture;
}

// Impl example
impl Component for PtyChannel {
    fn run(mut self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture {
        Box::pin(async move {
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        // Graceful shutdown: cleanup and return
                        self.close_pty().await?;
                        return Ok(());
                    }
                    line = read_stdin() => {
                        // Normal operation
                        self.handle_input(&line).await?;
                    }
                }
            }
        })
    }
}
```

**Key patterns:**
1. **CancellationToken:** Global shutdown signal to all components
2. **tokio::select!:** Each component respects shutdown in its main loop
3. **Cleanup on exit:** Component closes files/connections before returning
4. **No forced kills:** Allow components time to finish gracefully

---

## Error Handling & Resilience

### Pattern 1: Typed Errors (AppError enum)

**Araliya-Bot:**
```rust
#[derive(Debug)]
pub enum AppError {
    Config(String),
    Comms(String),
    Agent(String),
    Bus(String),
    // ...
}

impl Display for AppError { /* ... */ }
impl From<io::Error> for AppError { /* ... */ }
```

**Advantages:**
- ✅ Single error type; no `Box<dyn Error>`
- ✅ Pattern matching on error kinds
- ✅ Conversion traits minimize boilerplate

**Disadvantages:**
- ❌ All subsystems must contribute to enum
- ❌ Large enum if many error types

---

### Pattern 2: Error Context (anyhow-style)

**Alternative (not used in Araliya-Bot):**
```rust
use anyhow::{Context, Result};

async fn load_config() -> Result<Config> {
    let file = std::fs::read_to_string("config.toml")
        .context("failed to read config file")?;
    let cfg = toml::from_str(&file)
        .context("failed to parse config file")?;
    Ok(cfg)
}
```

**Advantages:**
- ✅ Error chaining (shows full context)
- ✅ Less boilerplate
- ✅ Works across crates

**Disadvantages:**
- ❌ Slower at runtime (string building)
- ❌ Less control over error types
- ❌ Pattern matching harder

---

### Pattern 3: Structured Errors

**Example:**
```rust
#[derive(Debug)]
pub struct BusError {
    code: i32,          // JSON-RPC error code
    message: String,    // Human-readable message
    data: Option<Value>,  // Optional context
}

impl BusError {
    pub fn new(code: i32, msg: impl Into<String>) -> Self {
        Self {
            code,
            message: msg.into(),
            data: None,
        }
    }
}
```

**Used in:** Araliya-Bot's JSON-RPC protocol
**Advantages:**
- ✅ Structured, parseable errors
- ✅ Additional context in `data` field
- ✅ Standard (JSON-RPC 2.0)

---

### Resilience: Retry Patterns

**Pattern 1: Exponential Backoff**
```rust
pub async fn with_retry<F, T>(mut f: F, max_retries: u32) -> Result<T>
where
    F: FnMut() -> Pin<Box<dyn Future<Output = Result<T>>>>,
{
    let mut retries = 0;
    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) if retries < max_retries => {
                let backoff_ms = 100 * 2_u64.pow(retries);
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                retries += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

**Pattern 2: Circuit Breaker**
```rust
pub struct CircuitBreaker {
    state: Arc<Mutex<CBState>>,
    failure_threshold: u32,
    reset_timeout: Duration,
}

enum CBState {
    Closed,                // Normal; forward requests
    Open(Instant),         // Failing; reject requests
    HalfOpen(Instant),     // Testing; allow one request
}

impl CircuitBreaker {
    pub async fn call<F>(&self, f: F) -> Result<T> {
        match *self.state.lock().unwrap() {
            CBState::Closed => { /* ... */ }
            CBState::Open(since) => {
                if since.elapsed() > self.reset_timeout {
                    *self.state.lock().unwrap() = CBState::HalfOpen(Instant::now());
                } else {
                    return Err("circuit open");
                }
            }
            CBState::HalfOpen(_) => { /* ... */ }
        }
    }
}
```

**When to use:**
- Exponential backoff: transient failures (network, temporary unavailability)
- Circuit breaker: downstream service is struggling (stop sending traffic)

**Araliya-Bot:** No retry logic currently. Could add for:
- LLM API calls (transient timeouts)
- External tool calls (Gmail, news API)

---

## Araliya-Bot Analysis

### Current Strengths

1. **Non-blocking supervisor:** Forward ownership of `reply_tx` and return immediately ✅
2. **Star topology:** No direct subsystem-to-subsystem coupling ✅
3. **CancellationToken shutdown:** Graceful signal to all components ✅
4. **Trait-based boundaries:** Agent, BusHandler, Component traits ✅
5. **Unified error handling:** Single AppError enum ✅
6. **Feature-gated modularity:** Compile-time subsystem selection ✅

### Improvement Opportunities

1. **Error Resilience:**
   - LLM calls: Add exponential backoff for API timeouts
   - Tool calls: Add retry logic with circuit breaker for external services
   - Session recovery: Checkpoint session state periodically

2. **Subsystem Recovery:**
   - Current: Subsystem error cancels all (panic recovery)
   - Improve: Allow agents subsystem to fail gracefully without killing supervisor
   - Strategy: Separate critical path (agents) from optional (tools, news)

3. **Observability:**
   - Add `detailed_status` handlers (beyond basic status)
   - Track subsystem health metrics (uptime, errors, latency)
   - Example:
     ```rust
     // agents/detailed_status returns:
     {
       "agents": {
         "echo": { "status": "on", "requests": 1000, "errors": 0 },
         "chat": { "status": "on", "requests": 500, "errors": 2 }
       }
     }
     ```

4. **Message Ordering:**
   - Currently: No ordering guarantees between async requests
   - Consider: Session-ordered messages (all operations for session_id are sequential)
   - Implementation: Per-session queue in agents subsystem

5. **Timeout Management:**
   - Add configurable request timeouts
   - Currently: Unbounded waits possible
   - Example:
     ```rust
     bus.call_with_timeout("agents/chat/handle", payload, Duration::from_secs(30)).await?
     ```

---

## Recommendations

### Short Term (Next Release)

1. **Add retry logic to external API calls:**
   ```rust
   // In tools subsystem
   async fn call_with_retry(
       &self,
       tool_id: &str,
       args: &str,
       max_retries: u32,
   ) -> Result<String> {
       with_exponential_backoff(
           || self.call_impl(tool_id, args),
           max_retries,
       ).await
   }
   ```

2. **Document subsystem initialization order** in CLAUDE.md or architecture guide

3. **Add integration test for shutdown sequence:**
   ```rust
   #[tokio::test]
   async fn test_graceful_shutdown() {
       let supervisor = start_supervisor().await;
       let shutdown = CancellationToken::new();
       shutdown.cancel();
       // Assert subsystems clean up in order
   }
   ```

### Medium Term (3-6 months)

1. **Add structured health reporting:**
   - Per-subsystem metrics (uptime, error count, latency)
   - Expose via `{prefix}/detailed_status` route
   - UI dashboard to visualize health

2. **Implement session-ordered message processing:**
   - Ensure operations on same session are sequential
   - Prevents race conditions in session state

3. **Add request timeouts:**
   - Configurable per-subsystem
   - Prevent unbounded waits
   - Circuit breaker for slow subsystems

### Long Term (6+ months)

1. **Graceful degradation for tools/news subsystems:**
   - Mark offline on first error
   - Allow agents to run without them
   - Manual reset via `manage/tools/reset` command

2. **Distributed tracing:**
   - Trace request flow: Agent → Memory → LLM
   - Add trace IDs to all messages
   - Export to OpenTelemetry for observability

3. **Multi-process architecture:**
   - Current: Single process, all subsystems
   - Future: Each subsystem as separate process (optional)
   - Message format already supports this (JSON-RPC over HTTP/IPC)

---

## Conclusion

Araliya-Bot's async supervision model is sound and follows industry best practices:

- ✅ Non-blocking supervisor (borrowed from actor model)
- ✅ Star topology (borrowed from Erlang OTP)
- ✅ Graceful shutdown with CancellationToken (Tokio idiom)
- ✅ Trait-based boundaries (Rust idiom)

The current design handles the bot's complexity well. Improvements should focus on resilience (retry logic, circuit breakers) and observability (health metrics, detailed status), not architectural changes.

---

**Document Version:** v1.0
**Last Updated:** 2026-03-19
