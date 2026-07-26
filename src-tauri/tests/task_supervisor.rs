use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use relay_pool_desktop_lib::background_tasks::{
    BoxTaskFuture, RestartPolicy, TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskState,
    TaskSupervisor, TaskSupervisorError,
};

fn immediate_task(
    result: Result<(), TaskFailure>,
) -> impl Fn(TaskRunContext) -> BoxTaskFuture + Send + Sync {
    move |_| {
        let result = result.clone();
        Box::pin(async move { result })
    }
}

#[tokio::test]
async fn rejects_duplicate_task_ids() {
    let supervisor = TaskSupervisor::new();
    supervisor
        .register(TaskSpec::new(
            "collector",
            "periodic",
            immediate_task(Ok(())),
        ))
        .expect("register task");

    let error = supervisor
        .register(TaskSpec::new(
            "collector",
            "periodic",
            immediate_task(Ok(())),
        ))
        .expect_err("duplicate id should fail");

    assert_eq!(
        error,
        TaskSupervisorError::DuplicateTaskId(TaskId::from("collector"))
    );
}

#[tokio::test]
async fn enforces_concurrency_key_non_reentry() {
    let supervisor = TaskSupervisor::new();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let release = Arc::new(tokio::sync::Mutex::new(Some(release_rx)));
    supervisor
        .register(
            TaskSpec::new("first", "runner", move |_| {
                let release = Arc::clone(&release);
                Box::pin(async move {
                    let rx = release.lock().await.take().expect("release receiver");
                    let _ = rx.await;
                    Ok(())
                })
            })
            .with_concurrency_key("station-collection"),
        )
        .expect("register first");
    supervisor
        .register(
            TaskSpec::new("second", "runner", immediate_task(Ok(())))
                .with_concurrency_key("station-collection"),
        )
        .expect("register second");

    supervisor
        .start(&TaskId::from("first"))
        .expect("start first");
    let error = supervisor
        .start(&TaskId::from("second"))
        .expect_err("second task should be blocked by concurrency key");
    assert_eq!(
        error,
        TaskSupervisorError::ConcurrencyKeyRunning("station-collection".to_string())
    );

    release_tx.send(()).expect("release running task");
    assert_eq!(
        supervisor
            .join_finished(&TaskId::from("first"))
            .await
            .unwrap(),
        TaskState::Succeeded
    );
    supervisor
        .start(&TaskId::from("second"))
        .expect("concurrency key released");
}

#[tokio::test]
async fn cancellation_is_visible_and_not_counted_as_failure() {
    let supervisor = TaskSupervisor::new();
    supervisor
        .register(TaskSpec::new("cancel-me", "runner", |context| {
            Box::pin(async move {
                context.cancellation_token.cancelled().await;
                Err(TaskFailure::cancelled())
            })
        }))
        .expect("register task");

    supervisor
        .start(&TaskId::from("cancel-me"))
        .expect("start task");
    supervisor
        .cancel(&TaskId::from("cancel-me"))
        .expect("cancel task");

    assert_eq!(
        supervisor
            .join_finished(&TaskId::from("cancel-me"))
            .await
            .unwrap(),
        TaskState::Cancelled
    );
    let status = supervisor.status(&TaskId::from("cancel-me")).unwrap();
    assert_eq!(status.consecutive_failures, 0);
}

#[tokio::test]
async fn transient_failure_schedules_deterministic_capped_retry() {
    let supervisor = TaskSupervisor::new();
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempt_counter = Arc::clone(&attempts);
    supervisor
        .register(
            TaskSpec::new("flaky", "runner", move |_| {
                let attempt_counter = Arc::clone(&attempt_counter);
                Box::pin(async move {
                    let attempt = attempt_counter.fetch_add(1, Ordering::SeqCst);
                    if attempt == 0 {
                        Err(TaskFailure::transient("temporary-network"))
                    } else {
                        Ok(())
                    }
                })
            })
            .with_restart_policy(RestartPolicy::transient(
                2,
                Duration::from_millis(50),
                Duration::from_millis(55),
            )),
        )
        .expect("register task");

    supervisor
        .start(&TaskId::from("flaky"))
        .expect("start task");
    assert_eq!(
        supervisor
            .join_finished(&TaskId::from("flaky"))
            .await
            .unwrap(),
        TaskState::BackingOff { retry_at_ms: 55 }
    );
    assert!(supervisor.tick(54).unwrap().is_empty());
    assert_eq!(supervisor.tick(55).unwrap().len(), 1);
    assert_eq!(
        supervisor
            .join_finished(&TaskId::from("flaky"))
            .await
            .unwrap(),
        TaskState::Succeeded
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn configuration_failure_and_panic_are_terminal() {
    let supervisor = TaskSupervisor::new();
    supervisor
        .register(TaskSpec::new(
            "bad-config",
            "runner",
            immediate_task(Err(TaskFailure::configuration("bad-config"))),
        ))
        .expect("register bad config");
    supervisor
        .register(TaskSpec::new("panic", "runner", |_| {
            Box::pin(async move {
                panic!("boom");
                #[allow(unreachable_code)]
                Ok(())
            })
        }))
        .expect("register panic");

    supervisor.start(&TaskId::from("bad-config")).unwrap();
    assert_eq!(
        supervisor
            .join_finished(&TaskId::from("bad-config"))
            .await
            .unwrap(),
        TaskState::Failed {
            code: "bad-config".to_string()
        }
    );

    supervisor.start(&TaskId::from("panic")).unwrap();
    assert_eq!(
        supervisor
            .join_finished(&TaskId::from("panic"))
            .await
            .unwrap(),
        TaskState::Panicked
    );
}

#[tokio::test]
async fn shutdown_cancels_tasks_and_reports_timeout_without_real_sleep() {
    tokio::time::pause();
    let supervisor = TaskSupervisor::new();
    supervisor
        .register(TaskSpec::new("stubborn", "runner", |_| {
            Box::pin(async move {
                std::future::pending::<()>().await;
                #[allow(unreachable_code)]
                Ok(())
            })
        }))
        .expect("register task");
    supervisor
        .start(&TaskId::from("stubborn"))
        .expect("start task");

    let shutdown = tokio::spawn(async move { supervisor.shutdown(Duration::from_secs(5)).await });
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(5)).await;

    let error = shutdown.await.expect("join shutdown").expect_err("timeout");
    assert_eq!(error.report.cancelled, vec![TaskId::from("stubborn")]);
    assert_eq!(error.report.timed_out, vec![TaskId::from("stubborn")]);
}
