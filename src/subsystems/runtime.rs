//! Generic subsystem runtime — shared scaffolding for all subsystems.
//!
//! # Component model
//!
//! A [`Component`] is any independently-runnable unit owned by a subsystem:
//! a comms channel (PTY, HTTP…), an agent plugin, a tool, etc.  
//! The subsystem constructs components with their shared state already
//! captured inside them, then hands them to [`spawn_components`].
//!
//! # SubsystemHandle
//!
//! [`spawn_components`] returns a [`SubsystemHandle`] that the caller can
//! `.await` (blocking until all components finish) or hold onto while doing
//! other work — the components run concurrently regardless.
//! Any component error cancels the shared [`CancellationToken`] so sibling
//! components and the supervisor all shut down cleanly.
//!
//! # Intra-subsystem events
//!
//! Each subsystem owns an `mpsc` channel for component → manager signalling
//! (e.g. "I finished", "session started"). This is kept out of the generic
//! runtime because the event type is subsystem-specific; subsystems wire it
//! up in their own `start()` function before calling [`spawn_components`].

use std::pin::Pin;
use std::future::Future;

use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};

use crate::error::AppError;

// ── Component ─────────────────────────────────────────────────────────────────

/// A boxed, owned future returned by [`Component::run`].
pub type ComponentFuture =
    Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + 'static>>;

/// A self-contained, concurrently-runnable unit owned by a subsystem.
///
/// Implementors capture all shared state (`Arc<XxxState>`, shutdown token, …)
/// at construction time. [`Component::run`] is called once by
/// [`spawn_components`] and should run until `shutdown` is cancelled or the
/// component's own work is done.
pub trait Component: Send + 'static {
    /// Stable identifier used in log messages.
    fn id(&self) -> &str;

    /// Consume the component and return its async run-loop as a boxed future.
    ///
    /// The returned future must be `Send + 'static` so it can be spawned on
    /// the Tokio thread pool. Capture the `CancellationToken` inside it to
    /// respect cooperative shutdown.
    fn run(self: Box<Self>, shutdown: CancellationToken) -> ComponentFuture;
}

// ── SubsystemHandle ───────────────────────────────────────────────────────────

/// An opaque handle to a running subsystem task set.
///
/// Returned by [`spawn_components`]. The caller can `.await` it to block until
/// all components have exited, or store it and poll it later.
pub struct SubsystemHandle {
    inner: JoinHandle<Result<(), AppError>>,
}

impl SubsystemHandle {
    /// Wrap an existing `JoinHandle` — used by subsystems that build their
    /// own manager task (e.g. comms, which also services an event queue).
    pub fn from_handle(handle: JoinHandle<Result<(), AppError>>) -> Self {
        Self { inner: handle }
    }

    /// Await all components and return the first error, if any.
    pub async fn join(self) -> Result<(), AppError> {
        match self.inner.await {
            Ok(r) => r,
            Err(e) => Err(AppError::Comms(format!("subsystem task panicked: {e}"))),
        }
    }
}

// ── spawn_components ──────────────────────────────────────────────────────────

/// Spawn each [`Component`] as an independent Tokio task and return a
/// [`SubsystemHandle`] that resolves when all components have exited.
///
/// Behaviour on error:
/// - If any component returns `Err`, `shutdown` is cancelled so all siblings
///   receive the cancellation signal and stop cooperatively.
/// - The manager task then drains the remaining components and returns the
///   first error encountered.
pub fn spawn_components(
    components: Vec<Box<dyn Component>>,
    shutdown: CancellationToken,
) -> SubsystemHandle {
    let handle = tokio::spawn(async move {
        let mut set: JoinSet<Result<(), AppError>> = JoinSet::new();

        for component in components {
            let id = component.id().to_string();
            let shutdown = shutdown.clone();
            debug!(component = %id, "spawning component");
            set.spawn(component.run(shutdown));
        }

        let mut first_err: Option<AppError> = None;

        while let Some(res) = set.join_next().await {
            match res {
                // Component panicked.
                Err(e) => {
                    error!("component panicked: {e}");
                    shutdown.cancel();
                    first_err.get_or_insert_with(|| {
                        AppError::Comms(format!("component panicked: {e}"))
                    });
                }
                // Component returned an error.
                Ok(Err(e)) => {
                    error!("component error: {e}");
                    shutdown.cancel();
                    first_err.get_or_insert(e);
                }
                // Component exited cleanly.
                Ok(Ok(())) => {}
            }
        }

        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    });

    SubsystemHandle { inner: handle }
}
