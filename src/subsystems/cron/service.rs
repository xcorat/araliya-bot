//! Background timer task — the cron service run-loop.
//!
//! Maintains a `BTreeMap<Instant, ScheduleEntry>` priority queue and sleeps
//! until the next deadline via `tokio::time::sleep_until`.  Zero polling.

use std::collections::BTreeMap;

use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, trace, warn};
use uuid::Uuid;

use crate::supervisor::bus::{BusHandle, BusPayload, CronEntryInfo, CronScheduleSpec};

// ── Commands ─────────────────────────────────────────────────────────────────

/// Internal command sent from the `BusHandler` impl to the background task.
pub enum CronCommand {
    Schedule {
        target_method: String,
        payload_json: String,
        spec: CronScheduleSpec,
        reply: oneshot::Sender<String>, // schedule_id
    },
    Cancel {
        schedule_id: String,
        reply: oneshot::Sender<bool>, // true if found and removed
    },
    List {
        reply: oneshot::Sender<Vec<CronEntryInfo>>,
    },
}

// ── Schedule entry ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ScheduleEntry {
    id: String,
    target_method: String,
    payload_json: String,
    spec: CronScheduleSpec,
}

// ── Service ──────────────────────────────────────────────────────────────────

/// The background timer service.  Created by `CronSubsystem::new` and spawned
/// as a tokio task.
pub struct CronService {
    bus: BusHandle,
    cmd_rx: mpsc::Receiver<CronCommand>,
    shutdown: CancellationToken,
}

impl CronService {
    pub fn new(
        bus: BusHandle,
        cmd_rx: mpsc::Receiver<CronCommand>,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            bus,
            cmd_rx,
            shutdown,
        }
    }

    /// Run the timer loop until shutdown.
    pub async fn run(mut self) {
        // Deadline → entry.  BTreeMap gives us O(log n) insert/remove and
        // O(1) access to the earliest deadline.  When two entries share the
        // same instant we nudge the later one by 1ns to keep keys unique.
        let mut queue: BTreeMap<Instant, ScheduleEntry> = BTreeMap::new();

        // Secondary index: schedule_id → deadline (for O(log n) cancel).
        let mut id_to_deadline: std::collections::HashMap<String, Instant> =
            std::collections::HashMap::new();

        info!("cron service running");

        loop {
            // Determine the next deadline (if any).
            let next_deadline = queue.keys().next().copied();

            tokio::select! {
                biased;

                // ── Shutdown ─────────────────────────────────────────────
                _ = self.shutdown.cancelled() => {
                    info!("cron service shutting down ({} active schedules dropped)", queue.len());
                    break;
                }

                // ── Incoming command ─────────────────────────────────────
                Some(cmd) = self.cmd_rx.recv() => {
                    match cmd {
                        CronCommand::Schedule { target_method, payload_json, spec, reply } => {
                            let id = Uuid::new_v4().to_string();
                            let deadline = spec_to_instant(&spec);
                            let entry = ScheduleEntry {
                                id: id.clone(),
                                target_method: target_method.clone(),
                                payload_json,
                                spec,
                            };
                            let deadline = insert_unique(&mut queue, deadline, entry);
                            id_to_deadline.insert(id.clone(), deadline);
                            debug!(schedule_id = %id, %target_method, ?deadline, "scheduled");
                            let _ = reply.send(id);
                        }
                        CronCommand::Cancel { schedule_id, reply } => {
                            let removed = if let Some(deadline) = id_to_deadline.remove(&schedule_id) {
                                queue.remove(&deadline);
                                debug!(%schedule_id, "cancelled");
                                true
                            } else {
                                debug!(%schedule_id, "cancel: not found");
                                false
                            };
                            let _ = reply.send(removed);
                        }
                        CronCommand::List { reply } => {
                            let entries: Vec<CronEntryInfo> = queue
                                .iter()
                                .map(|(deadline, entry)| {
                                    CronEntryInfo {
                                        schedule_id: entry.id.clone(),
                                        target_method: entry.target_method.clone(),
                                        spec: entry.spec.clone(),
                                        next_fire_unix_ms: instant_to_unix_ms(*deadline),
                                    }
                                })
                                .collect();
                            trace!(count = entries.len(), "listing schedules");
                            let _ = reply.send(entries);
                        }
                    }
                }

                // ── Timer fires ──────────────────────────────────────────
                _ = async {
                    match next_deadline {
                        Some(d) => tokio::time::sleep_until(d).await,
                        None => std::future::pending().await, // park forever
                    }
                } => {
                    // Pop the front entry (earliest deadline).
                    if let Some((deadline, entry)) = queue.pop_first() {
                        id_to_deadline.remove(&entry.id);

                        // Deserialize the stored payload.
                        let payload = match serde_json::from_str::<BusPayload>(&entry.payload_json) {
                            Ok(p) => p,
                            Err(e) => {
                                warn!(
                                    schedule_id = %entry.id,
                                    target = %entry.target_method,
                                    error = %e,
                                    "failed to deserialize cron payload — dropping entry"
                                );
                                continue;
                            }
                        };

                        debug!(
                            schedule_id = %entry.id,
                            target = %entry.target_method,
                            "cron firing"
                        );

                        // Emit the event as a bus notification.
                        if let Err(e) = self.bus.notify(&entry.target_method, payload) {
                            warn!(
                                schedule_id = %entry.id,
                                target = %entry.target_method,
                                error = %e,
                                "cron: failed to emit notification"
                            );
                        }

                        // Re-enqueue if repeating.
                        if let CronScheduleSpec::Interval { every_secs } = &entry.spec {
                            let next = deadline + std::time::Duration::from_secs(*every_secs);
                            let id = entry.id.clone();
                            let next = insert_unique(&mut queue, next, entry);
                            id_to_deadline.insert(id, next);
                        }
                    }
                }
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a [`CronScheduleSpec`] to a tokio [`Instant`].
fn spec_to_instant(spec: &CronScheduleSpec) -> Instant {
    match spec {
        CronScheduleSpec::Once { at_unix_ms } => {
            let target = std::time::UNIX_EPOCH
                + std::time::Duration::from_millis(*at_unix_ms);
            let now_sys = std::time::SystemTime::now();
            let now_inst = Instant::now();
            match target.duration_since(now_sys) {
                Ok(delta) => now_inst + delta,
                Err(_) => now_inst, // already in the past — fire immediately
            }
        }
        CronScheduleSpec::Interval { every_secs } => {
            Instant::now() + std::time::Duration::from_secs(*every_secs)
        }
    }
}

/// Insert into the BTreeMap, nudging the key by 1ns if it already exists
/// to guarantee unique keys.  Returns the actual key used.
fn insert_unique(
    queue: &mut BTreeMap<Instant, ScheduleEntry>,
    mut deadline: Instant,
    entry: ScheduleEntry,
) -> Instant {
    while queue.contains_key(&deadline) {
        deadline += std::time::Duration::from_nanos(1);
    }
    queue.insert(deadline, entry);
    deadline
}

/// Best-effort conversion from a tokio `Instant` to unix-epoch milliseconds.
/// Used only for the `cron/list` response — not for timer scheduling.
fn instant_to_unix_ms(instant: Instant) -> u64 {
    let now_inst = Instant::now();
    let now_sys = std::time::SystemTime::now();
    let unix_now = now_sys
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    if instant >= now_inst {
        let delta = instant - now_inst;
        (unix_now + delta).as_millis() as u64
    } else {
        let delta = now_inst - instant;
        unix_now.checked_sub(delta).map_or(0, |d| d.as_millis() as u64)
    }
}
