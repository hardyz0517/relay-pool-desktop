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
fn scheduler_handles_hundred_due_monitors_with_bounded_queue_and_no_catch_up_storm() {
    let mut scheduler = MonitoringScheduler::new(10);
    for index in 0..100 {
        scheduler.upsert_monitor(monitor(
            &format!("monitor-{index:03}"),
            &format!("key-{index:03}"),
            1_000,
        ));
    }

    let tick = scheduler.tick(5_000);
    assert_eq!(tick.admitted_count, 10);
    assert_eq!(tick.queue_full_count, 90);
    assert_eq!(tick.max_lag_ms, 4_000);
    assert!(
        tick.next_wakeup_at_ms.expect("next wakeup") >= 65_000,
        "scheduler should move every due monitor forward instead of catch-up storming"
    );

    let mut popped = Vec::new();
    while let Some(command) = scheduler.pop_ready() {
        popped.push(command);
    }
    assert_eq!(popped.len(), 10);
    assert!(popped
        .windows(2)
        .all(|pair| pair[0].monitor_id <= pair[1].monitor_id));
    assert_eq!(scheduler.diagnostics().queue_depth, 0);
    assert_eq!(scheduler.diagnostics().queue_full_count, 90);
}

#[test]
fn runtime_single_flights_same_station_key_manual_storm_and_drains_unique_key_load() {
    let runtime = MonitoringRuntime::new(MonitoringRuntimeLimits {
        queue_capacity: 128,
        global_concurrency: 8,
        station_concurrency: 4,
        key_concurrency: 1,
    });

    let RuntimeAdmission::Queued {
        execution_id: shared_execution,
        queue_depth,
    } = runtime.admit(command(
        "monitor-shared",
        "station-shared",
        &["key-shared"],
        MonitorTriggerKind::Scheduled,
        5_000,
    ))
    else {
        panic!("first shared command should queue");
    };
    assert_eq!(queue_depth, 1);
    for index in 0..100 {
        assert_eq!(
            runtime.admit(command(
                "monitor-shared",
                "station-shared",
                &["key-shared"],
                if index % 2 == 0 {
                    MonitorTriggerKind::Manual
                } else {
                    MonitorTriggerKind::Scheduled
                },
                0,
            )),
            RuntimeAdmission::Reused {
                execution_id: shared_execution.clone()
            }
        );
    }
    assert_eq!(runtime.diagnostics().queue_depth, 1);

    let RuntimeStart::Started { execution_id, .. } = runtime.start_next() else {
        panic!("shared execution should start");
    };
    runtime.guard(&execution_id).expect("shared guard").finish();
    assert_eq!(runtime.diagnostics().active_count, 0);

    for index in 0..100 {
        assert!(matches!(
            runtime.admit(command(
                &format!("monitor-{index:03}"),
                "station-unique",
                &[&format!("key-{index:03}")],
                MonitorTriggerKind::Scheduled,
                index,
            )),
            RuntimeAdmission::Queued { .. }
        ));
    }

    let mut started = 0usize;
    let mut active_guards = Vec::new();
    while started < 100 {
        match runtime.start_next() {
            RuntimeStart::Started { execution_id, .. } => {
                started += 1;
                active_guards.push(runtime.guard(&execution_id).expect("active guard"));
                assert!(
                    runtime.diagnostics().global_in_use <= 4,
                    "station permit should cap active load before global permit"
                );
            }
            RuntimeStart::PermitBlocked => {
                active_guards
                    .pop()
                    .expect("permit block requires at least one active execution")
                    .finish();
            }
            RuntimeStart::QueueEmpty => break,
            RuntimeStart::ShuttingDown => panic!("runtime should not shut down during drain"),
        }
    }
    for guard in active_guards {
        guard.finish();
    }

    assert_eq!(started, 100);
    assert_eq!(runtime.diagnostics().queue_depth, 0);
    assert_eq!(runtime.diagnostics().active_count, 0);
    assert_eq!(runtime.diagnostics().terminal_interrupted_count, 0);
}

fn monitor(id: &str, key_id: &str, next_due_at_ms: i64) -> ScheduledMonitor {
    ScheduledMonitor {
        monitor_id: id.to_string(),
        station_id: "station-1".to_string(),
        station_key_ids: vec![key_id.to_string()],
        next_due_at_ms,
        interval_ms: 60_000,
        jitter_ms: 0,
        schedule_revision: 1,
    }
}

fn command(
    monitor_id: &str,
    station_id: &str,
    station_key_ids: &[&str],
    trigger_kind: MonitorTriggerKind,
    lag_ms: i64,
) -> scheduler::SchedulerCommand {
    scheduler::SchedulerCommand {
        monitor_id: monitor_id.to_string(),
        station_id: station_id.to_string(),
        station_key_ids: station_key_ids.iter().map(|key| key.to_string()).collect(),
        trigger_kind,
        due_at_ms: 1_000,
        lag_ms,
        schedule_revision: 1,
    }
}
