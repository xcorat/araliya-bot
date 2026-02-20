# Cron Subsystem

**Status:** Implemented — `src/subsystems/cron/`  
**Feature:** `subsystem-cron`  
**Bus prefix:** `cron`

---

## Overview

The cron subsystem provides timer-based event scheduling. Other subsystems schedule events by sending bus requests; the cron service emits those events as bus notifications at the specified times. This keeps all inter-subsystem communication on the bus (star topology preserved).

---

## Architecture

### Module layout

```
src/subsystems/cron/
├── mod.rs       CronSubsystem — BusHandler, owns mpsc::Sender to bg task
└── service.rs   CronService   — background tokio task, priority queue, timer loop
```

### Internal communication

`CronSubsystem` (the handler) communicates with `CronService` (the background task) via an internal `mpsc` channel using `CronCommand`:

```rust
enum CronCommand {
    Schedule { id, target_method, payload_json, spec, reply },
    Cancel { id, reply },
    List { reply },
}
```

### Timer implementation

- **Priority queue:** `BTreeMap<Instant, ScheduleEntry>` — entries sorted by next fire time.
- **Secondary index:** `HashMap<String, Instant>` — schedule_id → key lookup for O(1) cancel.
- **Sleep strategy:** `tokio::time::sleep_until(next_deadline)` — no polling, no tick interval. When the queue is empty, the sleep branch is disabled via `std::future::pending()`.
- **Collision handling:** If two entries share the same `Instant`, the new entry is nudged forward by 1ns (`insert_unique`).

### Run loop

```rust
loop {
    tokio::select! {
        _ = shutdown.cancelled() => break,
        Some(cmd) = cmd_rx.recv() => { /* Schedule / Cancel / List */ },
        _ = sleep_until(next) => {
            // Fire notification via bus
            // Re-enqueue if Interval, remove if Once
        }
    }
}
```

---

## Bus Methods

### `cron/schedule` — Request

Schedule a new timed event.

**Payload:** `BusPayload::CronSchedule`

| Field | Type | Description |
|-------|------|-------------|
| `target_method` | `String` | Bus method to emit when the timer fires (e.g. `"agents/daily-digest"`) |
| `payload_json` | `String` | Serialized `BusPayload` to include in the notification |
| `spec` | `CronScheduleSpec` | Timing specification |

**`CronScheduleSpec` variants:**

| Variant | Fields | Behaviour |
|---------|--------|-----------|
| `Once` | `at_unix_ms: u64` | Fire once at the given UTC timestamp (ms), then remove |
| `Interval` | `every_secs: u64` | Fire repeatedly at the given interval from now |

**Reply:** `BusPayload::CronScheduleResult { schedule_id: String }`

### `cron/cancel` — Request

Cancel an active schedule.

**Payload:** `BusPayload::CronCancel { schedule_id: String }`

**Reply:** `BusPayload::Empty` on success, or `ERR_BAD_REQUEST` if the schedule_id was not found.

### `cron/list` — Request

List all active schedules.

**Payload:** `BusPayload::CronList`

**Reply:** `BusPayload::CronListResult { entries: Vec<CronEntryInfo> }`

| Field | Type | Description |
|-------|------|-------------|
| `schedule_id` | `String` | Unique identifier |
| `target_method` | `String` | Method that will be notified |
| `spec` | `CronScheduleSpec` | Original timing spec |
| `next_fire_unix_ms` | `u64` | Next fire time (UTC ms) |

---

## Event emission

When a timer fires, the cron service calls:

```rust
bus.notify(entry.target_method, BusPayload::Text(entry.payload_json))
```

The supervisor routes this notification by prefix to the appropriate subsystem. No special handling is needed — it looks like any other bus notification.

---

## Management integration

The management subsystem (`manage/http/get`) queries `cron/list` via the bus and includes `cron_active` (count) and `cron_schedules` (array) in the `main_process.details` section of the health JSON response.

The UI `StatusView` displays active cron schedules in the main process card with target method, spec type, and next fire countdown.

---

## Tests

4 unit tests in `service.rs` (use `tokio::test` with `start_paused = true`):

| Test | Validates |
|------|-----------|
| `schedule_and_list` | Schedule inserts correctly, List returns entry with correct fields |
| `cancel_success_and_miss` | Cancel removes entry, cancelling unknown ID returns error |
| `interval_fires_notification` | Interval timer fires notification on the bus at the right time |
| `once_fires_and_is_removed` | Once timer fires and is automatically removed from the queue |

---

## Design rationale

Approach A (full BusHandler) was chosen over side-channel or hybrid approaches. See [cron-service-design.md](../../../notes/implementation/cron-service-design.md) for the full comparison.

Key reasons:
- **Bus-native** — all scheduling goes through the bus, consistent with star topology
- **Discoverable** — any subsystem with a `BusHandle` can schedule events
- **Introspectable** — `cron/list` is available to management and HTTP adapters
- **Future-proof** — runtime-loaded plugins already have `BusHandle`, no extra wiring needed
