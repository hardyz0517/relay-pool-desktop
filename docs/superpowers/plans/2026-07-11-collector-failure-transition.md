# Collector Failure Transition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent repeated collector failures from recreating unread change notifications until that collector task recovers and fails again.

**Architecture:** Keep episode-state semantics in the SQLite change-event boundary. A `collector_failed` dedupe conflict preserves an active event, collector recovery resolves the matching failure key, and a later conflict reactivates the resolved row as a new unread episode.

**Tech Stack:** Rust, rusqlite, Tauri service tests

---

### Task 1: Preserve an active collector failure

**Files:**
- Modify: `src-tauri/src/services/database.rs:6975-7034`
- Test: `src-tauri/src/services/database.rs` test module

- [ ] **Step 1: Write the failing repeated-failure test**

Add a database test that inserts a failed `sub2api-groups` snapshot, marks its `collector_failed` event read, inserts another failed snapshot, and asserts the same event remains read with unchanged `detected_at` and `updated_at` values:

```rust
#[test]
fn repeated_collector_failure_preserves_active_event_read_state() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "collector-repeat-failure");

    database
        .insert_collector_snapshot(
            &station.id,
            "sub2api-groups",
            "failed",
            json!({}),
            json!({}),
            None,
            Some("first failure".to_string()),
        )
        .expect("first failed snapshot");
    let first = database
        .list_change_events()
        .expect("events")
        .into_iter()
        .find(|event| event.event_type == "collector_failed")
        .expect("collector failed event");
    let read = database
        .mark_change_event_read(first.id.clone())
        .expect("mark failure read");

    std::thread::sleep(std::time::Duration::from_millis(2));
    database
        .insert_collector_snapshot(
            &station.id,
            "sub2api-groups",
            "failed",
            json!({}),
            json!({}),
            None,
            Some("second failure".to_string()),
        )
        .expect("second failed snapshot");

    let repeated = database
        .list_change_events()
        .expect("events")
        .into_iter()
        .find(|event| event.event_type == "collector_failed")
        .expect("repeated collector failed event");
    assert_eq!(repeated.id, first.id);
    assert_eq!(repeated.status, "read");
    assert_eq!(repeated.detected_at, first.detected_at);
    assert_eq!(repeated.updated_at, read.updated_at);
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test repeated_collector_failure_preserves_active_event_read_state -- --nocapture`

Expected: FAIL because the repeated upsert returns status `unread` and refreshes timestamps.

- [ ] **Step 3: Add collector-specific upsert conflict rules**

In `upsert_change_event_in_connection`, extend the existing SQL `CASE` expressions so an unresolved `collector_failed` conflict preserves `status`, `detected_at`, `resolved_at`, and `updated_at`. A stored `resolved` event must continue through the existing `ELSE` branches so it becomes unread with fresh timestamps:

```sql
WHEN excluded.event_type = 'collector_failed'
 AND change_events.status != 'resolved'
THEN change_events.status
```

Apply the equivalent condition to the three timestamp fields, returning the corresponding stored column.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run: `cargo test repeated_collector_failure_preserves_active_event_read_state -- --nocapture`

Expected: PASS.

### Task 2: Resolve failure state on recovery

**Files:**
- Modify: `src-tauri/src/services/database.rs:7115-7130`
- Modify: `src-tauri/src/services/database.rs:9267-9278`
- Test: `src-tauri/src/services/database.rs` test module

- [ ] **Step 1: Write the failing recovery-cycle test**

Add a small run helper and a test that exercises the complete failure episode:

```rust
fn finish_test_collector_run(
    database: &AppDatabase,
    station_id: &str,
    task_type: &str,
    status: &str,
) {
    let run = database
        .create_collector_run(CreateCollectorRunInput {
            station_id: station_id.to_string(),
            parent_run_id: None,
            adapter: "sub2api".to_string(),
            task_type: task_type.to_string(),
        })
        .expect("create collector run");
    database
        .finish_collector_run(FinishCollectorRunInput {
            id: run.id,
            status: status.to_string(),
            endpoint_count: 1,
            success_count: i64::from(status == "success"),
            failure_count: i64::from(status == "failed"),
            manual_action_required: false,
            error_code: None,
            error_message: None,
            snapshot_id: None,
        })
        .expect("finish collector run");
}

#[test]
fn collector_failure_reactivates_after_recovery() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "collector-failure-recovery");
    database
        .insert_collector_snapshot(
            &station.id, "sub2api-groups", "failed", json!({}), json!({}), None,
            Some("first failure".to_string()),
        )
        .expect("failed snapshot");
    let first = database
        .list_change_events().expect("events").into_iter()
        .find(|event| event.event_type == "collector_failed")
        .expect("collector failed event");

    finish_test_collector_run(&database, &station.id, "groups", "failed");
    finish_test_collector_run(&database, &station.id, "groups", "success");
    let resolved = database
        .list_change_events().expect("events").into_iter()
        .find(|event| event.event_type == "collector_failed")
        .expect("resolved collector failed event");
    assert_eq!(resolved.status, "resolved");
    assert!(resolved.resolved_at.is_some());

    std::thread::sleep(std::time::Duration::from_millis(2));
    database
        .insert_collector_snapshot(
            &station.id, "sub2api-groups", "failed", json!({}), json!({}), None,
            Some("second failure".to_string()),
        )
        .expect("failed snapshot after recovery");
    let reactivated = database
        .list_change_events().expect("events").into_iter()
        .find(|event| event.event_type == "collector_failed")
        .expect("reactivated collector failed event");
    assert_eq!(reactivated.id, first.id);
    assert_eq!(reactivated.status, "unread");
    assert!(reactivated.resolved_at.is_none());
    assert_ne!(reactivated.detected_at, first.detected_at);
}
```

- [ ] **Step 2: Run the recovery-cycle test and verify RED**

Run: `cargo test collector_failure_reactivates_after_recovery -- --nocapture`

Expected: FAIL because recovery currently creates `collector_recovered` but leaves `collector_failed` active.

- [ ] **Step 3: Add a dedupe-key resolver**

Add a private helper that resolves an existing active event without failing when the key does not exist:

```rust
fn resolve_change_event_by_dedupe_key_in_connection(
    connection: &Connection,
    dedupe_key: &str,
) -> Result<(), String> {
    let now = now_string();
    connection
        .execute(
            "UPDATE change_events
                SET status = ?2, resolved_at = ?3, updated_at = ?3
              WHERE dedupe_key = ?1 AND status != ?2",
            params![dedupe_key, STATUS_RESOLVED, now],
        )
        .map_err(|error| format!("解决去重变更事件失败: {error}"))?;
    Ok(())
}
```

- [ ] **Step 4: Resolve the matching failure before emitting recovery**

In the failed-to-success/partial transition, derive the failure key with `collector_dedupe_key(&saved.station_id, "collector_failed", &saved.task_type)`, call the new resolver with `?`, then keep the existing `collector_recovered_event` upsert.

- [ ] **Step 5: Run the recovery-cycle test and verify GREEN**

Run: `cargo test collector_failure_reactivates_after_recovery -- --nocapture`

Expected: PASS.

### Task 3: Verify task isolation and the Rust surface

**Files:**
- Test: `src-tauri/src/services/database.rs` test module

- [ ] **Step 1: Add the task-isolation regression test**

Create active `groups` and `balance` failure events for one station, recover `groups`, and assert task isolation:

```rust
#[test]
fn collector_failure_recovery_is_scoped_to_task_type() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "collector-failure-task-scope");
    for task_type in ["groups", "balance"] {
        database
            .insert_collector_snapshot(
                &station.id,
                &format!("sub2api-{task_type}"),
                "failed",
                json!({}),
                json!({}),
                None,
                Some(format!("{task_type} failure")),
            )
            .expect("failed snapshot");
    }

    finish_test_collector_run(&database, &station.id, "groups", "failed");
    finish_test_collector_run(&database, &station.id, "groups", "success");

    let failures = database
        .list_change_events().expect("events").into_iter()
        .filter(|event| event.event_type == "collector_failed")
        .map(|event| (event.dedupe_key, event.status))
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        failures.get(&crate::services::change_events::collector_dedupe_key(
            &station.id, "collector_failed", "groups"
        )),
        Some(&"resolved".to_string())
    );
    assert_eq!(
        failures.get(&crate::services::change_events::collector_dedupe_key(
            &station.id, "collector_failed", "balance"
        )),
        Some(&"unread".to_string())
    );
}
```

- [ ] **Step 2: Run all collector failure transition tests**

Run: `cargo test collector_failure -- --nocapture`

Expected: all matching tests PASS.

- [ ] **Step 3: Format and run Rust verification**

Run: `cargo fmt --check`

Expected: exit code 0.

Run: `cargo test`

Expected: exit code 0.

Run: `cargo check`

Expected: exit code 0.

- [ ] **Step 4: Commit only the collector lifecycle implementation**

```powershell
git add -- src-tauri/src/services/database.rs docs/superpowers/plans/2026-07-11-collector-failure-transition.md
git commit -m "fix: dedupe repeated collector failures"
```
