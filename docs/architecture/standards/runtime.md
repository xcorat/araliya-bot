# Component Runtime

**Status:** Implemented — `src/subsystems/runtime.rs`

The component runtime is the generic scaffolding shared by all subsystems. It defines how independently-runnable units are structured, spawned, and shut down.

---

## `Component` trait

```rust
pub trait Component: Send + 'static {
    fn id(&self) -> &str;
    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture;
}

pub type ComponentFuture =
    Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'static>>;
```

A `Component` is any independently-runnable unit owned by a subsystem: a comms channel (PTY, HTTP), an agent plugin wrapper, a tool runner, etc.

**Construction contract:** components capture all shared state (`Arc<XxxState>`, `BusHandle`, configuration) at construction time — not at `run` time. `run` takes only `self` (by value, boxed) and a `CancellationToken`. There are no mutable references to shared state after construction.

**`run` contract:**
- Called exactly once by `spawn_components`.
- Must run until `shutdown` is cancelled or the component's own work is complete.
- Must return `Err(AppError)` on failure; the error propagates to trigger sibling cancellation.
- Must be `Send + 'static` — no borrowed references that outlive the call.
- Must not block a Tokio thread — use `.await` for I/O, `tokio::task::spawn_blocking` for CPU-bound work.

`ComponentFuture` is a `Pin<Box<dyn Future>>` type alias so the trait is object-safe on stable Rust without `async-trait`.

---

## `spawn_components`

```rust
pub fn spawn_components(
    components: Vec<Box<dyn Component>>,
    shutdown: CancellationToken,
) -> SubsystemHandle
```

Takes ownership of all components for a subsystem and spawns each as an independent Tokio task. Returns a `SubsystemHandle` immediately — components run concurrently as soon as they are spawned.

### Error and cancellation behaviour (fail-fast)

1. Any component that returns `Err` cancels the shared `CancellationToken`.
2. All sibling components (and the supervisor, which shares the same token) receive the signal and stop cooperatively.
3. The internal manager task drains the remaining join handles and returns the **first error** encountered.

This ensures the system never continues running in a partially-failed state.

### Lifecycle

```
subsystem::start()
  ├─ construct Component instances (capture Arc<State>, BusHandle, config)
  ├─ spawn_components(components, shutdown_token)  → SubsystemHandle
  │   └─ per Component: tokio::spawn(component.run(shutdown_token.clone()))
  │
  │   [components run concurrently]
  │
  ├─ on any component Err: token.cancel() → siblings receive cancellation signal
  └─ SubsystemHandle::join().await → first Err, or Ok(())
```

---

## `SubsystemHandle`

```rust
pub struct SubsystemHandle {
    inner: JoinHandle<Result<(), AppError>>,
}

impl SubsystemHandle {
    pub async fn join(self) -> Result<(), AppError>;
    pub fn from_handle(handle: JoinHandle<Result<(), AppError>>) -> Self;  // escape hatch
}
```

An opaque handle to a running subsystem. `join()` blocks until all components have exited. `from_handle` is an escape hatch for subsystems that build a custom manager task outside of `spawn_components`.

---

## Intra-subsystem events

Each subsystem may maintain its own `mpsc` channel for component-to-manager signalling (e.g. "session started", "channel shutdown"). This is kept **out of the generic runtime** because the event type is subsystem-specific. Subsystems wire it up in their own `start()` function before calling `spawn_components`.

See `subsystems/comms/state.rs` (`CommsEvent`) for a reference implementation.
