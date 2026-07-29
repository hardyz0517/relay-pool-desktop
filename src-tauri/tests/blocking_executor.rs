use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

use relay_pool_desktop_lib::background_tasks::{
    BlockingExecutor, BlockingExecutorConfig, BlockingExecutorError,
};

fn test_executor(max_running: usize, queue_capacity: usize) -> BlockingExecutor {
    BlockingExecutor::new(BlockingExecutorConfig {
        max_running,
        queue_capacity,
        queue_timeout: Duration::from_millis(10),
        default_execution_timeout: Duration::from_millis(10),
    })
}

fn configured_executor(
    max_running: usize,
    queue_capacity: usize,
    queue_timeout: Duration,
    execution_timeout: Duration,
) -> BlockingExecutor {
    BlockingExecutor::new(BlockingExecutorConfig {
        max_running,
        queue_capacity,
        queue_timeout,
        default_execution_timeout: execution_timeout,
    })
}

async fn wait_for_running(executor: &BlockingExecutor, running: usize) {
    for _ in 0..100 {
        if executor.metrics().running == running {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("executor did not reach running={running}");
}

#[tokio::test]
async fn rejects_when_queue_capacity_is_full() {
    let executor = test_executor(1, 1);
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let first = executor
        .submit("dialog", None, None, None, move |_| {
            release_rx.recv().expect("release first job");
            Ok("first")
        })
        .expect("first job admitted");
    wait_for_running(&executor, 1).await;

    let queued = executor
        .submit("dialog", None, None, None, |_| Ok("queued"))
        .expect("second job admitted to queue");
    let rejected = executor
        .submit("dialog", None, None, None, |_| Ok("rejected"))
        .expect_err("third job should exceed bounded queue");

    assert_eq!(rejected, BlockingExecutorError::QueueFull);
    release_tx.send(()).expect("release first");
    assert_eq!(first.result().await.unwrap(), "first");
    assert_eq!(queued.result().await.unwrap(), "queued");
}

#[tokio::test]
async fn queued_job_times_out_without_real_sleep() {
    let executor = configured_executor(1, 2, Duration::ZERO, Duration::from_millis(1_000));
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let first = executor
        .submit("keyring", None, None, None, move |_| {
            release_rx.recv().expect("release first job");
            Ok(())
        })
        .expect("first job admitted");
    wait_for_running(&executor, 1).await;
    let queued = executor
        .submit("keyring", None, None, None, |_| Ok(()))
        .expect("queued job admitted");

    tokio::task::yield_now().await;
    assert_eq!(
        queued
            .result()
            .await
            .expect_err("queued job should time out"),
        BlockingExecutorError::QueueTimeout
    );

    release_tx.send(()).expect("release first");
    first.result().await.expect("first succeeds");
}

#[tokio::test]
async fn queued_job_can_be_cancelled_before_start() {
    let executor = test_executor(1, 2);
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let first = executor
        .submit("filesystem", None, None, None, move |_| {
            release_rx.recv().expect("release first job");
            Ok(())
        })
        .expect("first job admitted");
    wait_for_running(&executor, 1).await;
    let queued = executor
        .submit("filesystem", None, None, None, |_| Ok(()))
        .expect("queued job admitted");

    queued.cancel();
    assert_eq!(
        queued
            .result()
            .await
            .expect_err("queued cancel should be terminal"),
        BlockingExecutorError::CancelledBeforeStart
    );
    release_tx.send(()).expect("release first");
    first.result().await.expect("first succeeds");
}

#[tokio::test]
async fn cancellation_discards_late_uncancellable_result() {
    let executor = test_executor(1, 1);
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let job = executor
        .submit(
            "dialog",
            Some("op-1".to_string()),
            Some("corr-1".to_string()),
            None,
            move |context| {
                assert_eq!(context.kind, "dialog");
                assert_eq!(context.operation_id.as_deref(), Some("op-1"));
                assert_eq!(context.correlation_id.as_deref(), Some("corr-1"));
                release_rx.recv().expect("release blocking job");
                Ok("late")
            },
        )
        .expect("job admitted");
    wait_for_running(&executor, 1).await;

    job.cancel();
    release_tx.send(()).expect("release job");
    assert_eq!(
        job.result()
            .await
            .expect_err("late result should be discarded"),
        BlockingExecutorError::CancelledLateResultDiscarded
    );
}

#[tokio::test]
async fn execution_timeout_reports_orphan_until_physical_call_returns() {
    let executor = configured_executor(1, 1, Duration::from_millis(10), Duration::ZERO);
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let job = executor
        .submit("keyring", None, None, None, move |_| {
            release_rx.recv().expect("release orphaned job");
            Ok(())
        })
        .expect("job admitted");
    wait_for_running(&executor, 1).await;

    tokio::task::yield_now().await;
    assert_eq!(
        job.result().await.expect_err("execution should time out"),
        BlockingExecutorError::ExecutionTimeout
    );
    assert_eq!(executor.metrics().orphaned, 1);
    release_tx.send(()).expect("release orphaned job");
}

#[tokio::test]
async fn panic_is_typed_failure_and_parent_deadline_caps_queue_wait() {
    let executor = configured_executor(
        1,
        2,
        Duration::from_millis(10),
        Duration::from_millis(1_000),
    );
    let panic_job = executor
        .submit(
            "panic",
            None,
            None,
            None,
            |_| -> Result<(), BlockingExecutorError> {
                panic!("blocking panic");
            },
        )
        .expect("panic job admitted");
    assert_eq!(
        panic_job.result().await.expect_err("panic should be typed"),
        BlockingExecutorError::Panicked
    );

    let (release_tx, release_rx) = mpsc::channel::<()>();
    let first = executor
        .submit("deadline", None, None, None, move |_| {
            release_rx.recv().expect("release first job");
            Ok(())
        })
        .expect("first job admitted");
    wait_for_running(&executor, 1).await;
    let queued = executor
        .submit("deadline", None, None, Some(Instant::now()), |_| Ok(()))
        .expect("queued job admitted");
    tokio::task::yield_now().await;
    assert_eq!(
        queued
            .result()
            .await
            .expect_err("parent deadline caps queue wait"),
        BlockingExecutorError::QueueTimeout
    );
    release_tx.send(()).expect("release first job");
    first.result().await.expect("first succeeds");
}
