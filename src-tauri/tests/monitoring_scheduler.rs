#[path = "../src/services/monitoring/runtime.rs"]
mod runtime;
#[path = "../src/services/monitoring/scheduler.rs"]
mod scheduler;

mod background_tasks {
    pub use relay_pool_desktop_lib::background_tasks::*;
}

use runtime::{MonitoringRuntime, MonitoringRuntimeLimits, RuntimeAdmission, RuntimeStart};
use scheduler::{MonitorTriggerKind, MonitoringScheduler, ScheduledMonitor};

#[test]
fn scheduler_uses_nearest_due_edit_notifications_startup_stagger_and_forward_jitter() {
    let mut scheduler = MonitoringScheduler::new(8);
    let entries = MonitoringScheduler::startup_stagger(
        [
            monitor("monitor-b", 900, 1_000, 0),
            monitor("monitor-a", 900, 1_000, 0),
        ],
        1_000,
        250,
    );
    assert_eq!(entries[0].monitor_id, "monitor-a");
    assert_eq!(entries[0].next_due_at_ms, 1_000);
    assert_eq!(entries[1].monitor_id, "monitor-b");
    assert_eq!(entries[1].next_due_at_ms, 1_250);

    for entry in entries {
        scheduler.upsert_monitor(entry);
    }
    assert_eq!(scheduler.next_wakeup_at_ms(), Some(1_000));

    let tick = scheduler.tick(999);
    assert_eq!(tick.admitted_count, 0);
    assert_eq!(tick.next_wakeup_at_ms, Some(1_000));

    let tick = scheduler.tick(1_000);
    assert_eq!(tick.admitted_count, 1);
    assert_eq!(scheduler.diagnostics().queue_depth, 1);
    let command = scheduler.pop_ready().expect("scheduled command");
    assert_eq!(command.monitor_id, "monitor-a");
    assert_eq!(command.trigger_kind, MonitorTriggerKind::Scheduled);
    assert_eq!(command.lag_ms, 0);
    assert!(scheduler.next_wakeup_at_ms().expect("next due") >= 1_250);

    scheduler.notify_definition_edit(monitor("monitor-a", 500, 1_000, 0), 1_100);
    assert_eq!(scheduler.next_wakeup_at_ms(), Some(1_100));

    let mut jittered = MonitoringScheduler::new(4);
    jittered.upsert_monitor(monitor("monitor-jitter", 1_000, 1_000, 500));
    jittered.tick(1_000);
    let next = jittered.next_wakeup_at_ms().expect("jittered next due");
    assert!(next >= 2_000);
    assert!(next <= 2_500);
}

#[test]
fn manual_execution_does_not_move_scheduled_baseline_or_cause_catch_up_storm() {
    let mut scheduler = MonitoringScheduler::new(8);
    scheduler.upsert_monitor(monitor("monitor-a", 1_000, 1_000, 0));
    let manual = scheduler
        .enqueue_manual("monitor-a", 900)
        .expect("manual command");
    assert_eq!(manual.trigger_kind, MonitorTriggerKind::Manual);
    assert_eq!(scheduler.next_wakeup_at_ms(), Some(1_000));

    let tick = scheduler.tick(10_000);
    assert_eq!(tick.admitted_count, 1);
    assert_eq!(tick.max_lag_ms, 9_000);
    assert_eq!(
        scheduler
            .pop_ready()
            .expect("only one scheduled catch-up")
            .due_at_ms,
        1_000
    );
    assert_eq!(scheduler.pop_ready(), None);
    assert_eq!(scheduler.next_wakeup_at_ms(), Some(11_000));
}

#[test]
fn runtime_single_flights_monitor_and_station_keys_across_manual_and_scheduled() {
    let runtime = MonitoringRuntime::new(MonitoringRuntimeLimits {
        queue_capacity: 4,
        global_concurrency: 2,
        station_concurrency: 2,
        key_concurrency: 1,
    });

    let scheduled = command("monitor-a", &["key-a"], MonitorTriggerKind::Scheduled, 25);
    let RuntimeAdmission::Queued { execution_id, .. } = runtime.admit(scheduled) else {
        panic!("scheduled should queue");
    };
    assert_eq!(
        runtime.admit(command(
            "monitor-a",
            &["key-a"],
            MonitorTriggerKind::Manual,
            0
        )),
        RuntimeAdmission::Reused {
            execution_id: execution_id.clone()
        }
    );
    assert_eq!(
        runtime.admit(command(
            "monitor-b",
            &["key-a"],
            MonitorTriggerKind::Scheduled,
            0
        )),
        RuntimeAdmission::Reused {
            execution_id: execution_id.clone()
        }
    );

    let RuntimeStart::Started {
        execution_id: started,
        ..
    } = runtime.start_next()
    else {
        panic!("queued execution should start");
    };
    assert_eq!(started, execution_id);
    let guard = runtime.guard(&started).expect("execution guard");
    assert_eq!(
        runtime.admit(command(
            "monitor-a",
            &["key-a"],
            MonitorTriggerKind::Manual,
            0
        )),
        RuntimeAdmission::Reused {
            execution_id: started.clone()
        }
    );
    guard.finish();

    assert!(matches!(
        runtime.admit(command(
            "monitor-b",
            &["key-a"],
            MonitorTriggerKind::Manual,
            0
        )),
        RuntimeAdmission::Queued { .. }
    ));
}

#[test]
fn runtime_enforces_global_station_key_permits_with_raii_release() {
    let runtime = MonitoringRuntime::new(MonitoringRuntimeLimits {
        queue_capacity: 8,
        global_concurrency: 2,
        station_concurrency: 1,
        key_concurrency: 1,
    });
    assert!(matches!(
        runtime.admit(command(
            "monitor-a",
            &["key-a"],
            MonitorTriggerKind::Scheduled,
            0
        )),
        RuntimeAdmission::Queued { .. }
    ));
    assert!(matches!(
        runtime.admit(command(
            "monitor-b",
            &["key-b"],
            MonitorTriggerKind::Scheduled,
            0
        )),
        RuntimeAdmission::Queued { .. }
    ));

    let RuntimeStart::Started {
        execution_id: first,
        ..
    } = runtime.start_next()
    else {
        panic!("first should start");
    };
    let first_guard = runtime.guard(&first).expect("first guard");
    assert_eq!(runtime.start_next(), RuntimeStart::PermitBlocked);
    assert_eq!(runtime.diagnostics().global_in_use, 1);

    drop(first_guard);
    let RuntimeStart::Started {
        execution_id: second,
        ..
    } = runtime.start_next()
    else {
        panic!("second should start after guard drop");
    };
    runtime.guard(&second).expect("second guard").finish();
    assert_eq!(runtime.diagnostics().global_in_use, 0);
}

#[test]
fn runtime_records_queue_full_lag_and_shutdown_cancels_queue_then_interrupts_running() {
    let runtime = MonitoringRuntime::new(MonitoringRuntimeLimits {
        queue_capacity: 1,
        global_concurrency: 1,
        station_concurrency: 1,
        key_concurrency: 1,
    });
    assert!(matches!(
        runtime.admit(command(
            "monitor-a",
            &["key-a"],
            MonitorTriggerKind::Scheduled,
            15
        )),
        RuntimeAdmission::Queued { .. }
    ));
    assert_eq!(
        runtime.admit(command(
            "monitor-b",
            &["key-b"],
            MonitorTriggerKind::Scheduled,
            30
        )),
        RuntimeAdmission::QueueFull { lag_ms: 30 }
    );
    assert_eq!(runtime.diagnostics().queue_full_count, 1);
    assert_eq!(runtime.diagnostics().max_lag_ms, 30);

    let RuntimeStart::Started { execution_id, .. } = runtime.start_next() else {
        panic!("queued execution should start");
    };
    let guard = runtime.guard(&execution_id).expect("running guard");
    assert_eq!(runtime.diagnostics().active_count, 1);
    assert!(matches!(
        runtime.admit(command(
            "monitor-c",
            &["key-c"],
            MonitorTriggerKind::Scheduled,
            0
        )),
        RuntimeAdmission::Queued { .. }
    ));

    let shutdown = runtime.shutdown_begin();
    assert_eq!(shutdown.queued_cancelled, 1);
    assert_eq!(shutdown.running_to_interrupt, vec![execution_id.clone()]);
    assert_eq!(
        runtime.admit(command(
            "monitor-d",
            &["key-d"],
            MonitorTriggerKind::Manual,
            0
        )),
        RuntimeAdmission::ShuttingDown
    );
    assert_eq!(runtime.interrupt_running(), 1);
    assert_eq!(runtime.diagnostics().terminal_interrupted_count, 1);
    drop(guard);
    assert_eq!(runtime.diagnostics().active_count, 0);
}

#[tokio::test]
async fn runtime_registers_with_task_supervisor_and_interrupts_on_cancellation() {
    let supervisor = background_tasks::TaskSupervisor::new();
    let runtime = MonitoringRuntime::new(MonitoringRuntimeLimits {
        queue_capacity: 4,
        global_concurrency: 1,
        station_concurrency: 1,
        key_concurrency: 1,
    });
    let task_id =
        runtime::register_monitoring_runtime_task(&supervisor, runtime.clone()).expect("register");

    supervisor.start(&task_id).expect("start runtime task");
    let RuntimeAdmission::Queued { .. } = runtime.admit(command(
        "monitor-a",
        &["key-a"],
        MonitorTriggerKind::Scheduled,
        0,
    )) else {
        panic!("execution should queue");
    };
    let RuntimeStart::Started { execution_id, .. } = runtime.start_next() else {
        panic!("execution should start");
    };
    let guard = runtime.guard(&execution_id).expect("running guard");

    supervisor.cancel(&task_id).expect("cancel runtime task");
    assert_eq!(
        supervisor.join_finished(&task_id).await.expect("join"),
        background_tasks::TaskState::Cancelled
    );
    assert!(!runtime.diagnostics().admitting);
    assert_eq!(runtime.diagnostics().terminal_interrupted_count, 1);
    drop(guard);
    assert_eq!(runtime.diagnostics().active_count, 0);
}

fn monitor(id: &str, next_due_at_ms: i64, interval_ms: i64, jitter_ms: i64) -> ScheduledMonitor {
    ScheduledMonitor {
        monitor_id: id.to_string(),
        station_id: "station-1".to_string(),
        station_key_ids: vec![format!("key-{id}")],
        next_due_at_ms,
        interval_ms,
        jitter_ms,
        schedule_revision: 1,
    }
}

fn command(
    monitor_id: &str,
    station_key_ids: &[&str],
    trigger_kind: MonitorTriggerKind,
    lag_ms: i64,
) -> scheduler::SchedulerCommand {
    scheduler::SchedulerCommand {
        monitor_id: monitor_id.to_string(),
        station_id: "station-1".to_string(),
        station_key_ids: station_key_ids.iter().map(|key| key.to_string()).collect(),
        trigger_kind,
        due_at_ms: 1_000,
        lag_ms,
        schedule_revision: 1,
    }
}
