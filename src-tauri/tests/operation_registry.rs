use std::time::{Duration, Instant};

use relay_pool_desktop_lib::background_tasks::{
    BoxOperationFuture, CancellationPolicy, OperationCancelOutcome, OperationContext,
    OperationFailureCode, OperationOwner, OperationRegistry, OperationRegistryConfig,
    OperationRegistryError, OperationStartRequest, OperationState, OperationTerminal,
};

fn registry_config() -> OperationRegistryConfig {
    OperationRegistryConfig {
        max_running_global: 2,
        max_running_per_concurrency_key: 1,
        progress_ring_entries_per_operation: 3,
        progress_entry_max_bytes: 32,
        terminal_ttl: Duration::from_millis(100),
        terminal_max_entries: 2,
        expired_tombstone_ttl: Duration::from_secs(60),
        default_deadline: Duration::from_secs(5),
    }
}

fn registry() -> OperationRegistry {
    OperationRegistry::new(registry_config())
}

fn owner() -> OperationOwner {
    OperationOwner::new("key-pool")
}

fn immediate(
    terminal: OperationTerminal,
) -> impl FnOnce(OperationContext) -> BoxOperationFuture + Send + 'static {
    move |_| Box::pin(async move { terminal })
}

async fn wait_for_terminal(
    registry: &OperationRegistry,
    id: relay_pool_desktop_lib::background_tasks::OperationId,
) -> OperationTerminal {
    for _ in 0..100 {
        if let Some(terminal) = registry.status(id).expect("operation status").terminal {
            return terminal;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    panic!("operation did not reach terminal");
}

#[tokio::test]
async fn start_returns_unique_id_and_progress_carries_same_id() {
    let registry = registry();
    let id = registry
        .start(OperationStartRequest::new(
            "connectivity",
            owner(),
            |context| {
                Box::pin(async move {
                    assert_eq!(context.kind, "connectivity");
                    assert_eq!(context.owner.feature, "key-pool");
                    context.emit_progress("queued").unwrap();
                    context.emit_progress("probing").unwrap();
                    OperationTerminal::Completed
                })
            },
        ))
        .expect("operation starts");

    assert!(id.as_u64() > 0);
    assert_eq!(
        wait_for_terminal(&registry, id).await,
        OperationTerminal::Completed
    );
    let snapshot = registry.status(id).expect("terminal snapshot");
    assert_eq!(snapshot.id, id);
    assert_eq!(snapshot.kind, "connectivity");
    assert_eq!(snapshot.progress.len(), 2);
    assert!(snapshot
        .progress
        .iter()
        .all(|progress| progress.id == id && progress.sequence > 0));
    assert!(matches!(snapshot.state, OperationState::Terminal { .. }));
}

#[tokio::test]
async fn operation_context_carries_bounded_correlation_id() {
    let registry = registry();
    let (correlation_tx, correlation_rx) = tokio::sync::oneshot::channel::<String>();
    let id = registry
        .start(OperationStartRequest::new(
            "connectivity",
            owner(),
            move |context| {
                Box::pin(async move {
                    correlation_tx
                        .send(context.correlation_id)
                        .expect("send correlation id");
                    OperationTerminal::Completed
                })
            },
        ))
        .expect("operation starts");

    assert_eq!(
        wait_for_terminal(&registry, id).await,
        OperationTerminal::Completed
    );
    let correlation_id = correlation_rx.await.expect("correlation id");

    assert_eq!(correlation_id.len(), 32);
    assert!(correlation_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
}

#[tokio::test]
async fn admission_is_atomic_for_capacity_and_concurrency_key() {
    let registry = registry();
    let (release_first_tx, release_first_rx) = tokio::sync::oneshot::channel::<()>();
    let first_release = std::sync::Arc::new(tokio::sync::Mutex::new(Some(release_first_rx)));
    let first = registry
        .start(
            OperationStartRequest::new("scan", owner(), {
                let first_release = std::sync::Arc::clone(&first_release);
                move |_| {
                    Box::pin(async move {
                        let rx = first_release.lock().await.take().expect("release receiver");
                        let _ = rx.await;
                        OperationTerminal::Completed
                    })
                }
            })
            .with_concurrency_key("station-1"),
        )
        .expect("first starts");

    let conflict = registry
        .start(
            OperationStartRequest::new("scan", owner(), |_| {
                Box::pin(async { OperationTerminal::Completed })
            })
            .with_concurrency_key("station-1"),
        )
        .expect_err("same concurrency key must fail before spawn");
    assert_eq!(
        conflict,
        OperationRegistryError::Conflict {
            concurrency_key: "station-1".to_string()
        }
    );

    let (release_second_tx, release_second_rx) = tokio::sync::oneshot::channel::<()>();
    let second_release = std::sync::Arc::new(tokio::sync::Mutex::new(Some(release_second_rx)));
    let second = registry
        .start(OperationStartRequest::new("capture", owner(), {
            let second_release = std::sync::Arc::clone(&second_release);
            move |_| {
                Box::pin(async move {
                    let rx = second_release
                        .lock()
                        .await
                        .take()
                        .expect("release receiver");
                    let _ = rx.await;
                    OperationTerminal::Completed
                })
            }
        }))
        .expect("second starts");
    assert_eq!(
        registry
            .start(OperationStartRequest::new("overflow", owner(), |_| {
                Box::pin(async { OperationTerminal::Completed })
            }))
            .expect_err("global running capacity must fail before spawn"),
        OperationRegistryError::Overloaded
    );
    assert_eq!(registry.metrics().running, 2);

    release_first_tx.send(()).expect("release first");
    release_second_tx.send(()).expect("release second");
    assert_eq!(
        wait_for_terminal(&registry, first).await,
        OperationTerminal::Completed
    );
    assert_eq!(
        wait_for_terminal(&registry, second).await,
        OperationTerminal::Completed
    );
    assert_eq!(registry.metrics().running, 0);
}

#[tokio::test]
async fn progress_ring_is_bounded_and_terminal_has_separate_status_path() {
    let registry = registry();
    let id = registry
        .start(OperationStartRequest::new(
            "connectivity",
            owner(),
            |context| {
                Box::pin(async move {
                    for step in ["one", "two", "three", "four", "five"] {
                        context.emit_progress(step).unwrap();
                    }
                    OperationTerminal::Failed {
                        code: OperationFailureCode::new("provider-timeout"),
                    }
                })
            },
        ))
        .expect("operation starts");

    assert_eq!(
        wait_for_terminal(&registry, id).await,
        OperationTerminal::Failed {
            code: OperationFailureCode::new("provider-timeout")
        }
    );
    let snapshot = registry.status(id).expect("terminal snapshot");
    assert_eq!(
        snapshot
            .progress
            .iter()
            .map(|progress| progress.message.as_str())
            .collect::<Vec<_>>(),
        ["three", "four", "five"]
    );
    assert!(snapshot.terminal.is_some());
    assert_eq!(
        registry
            .progress(id, "late")
            .expect_err("terminal blocks progress"),
        OperationRegistryError::TerminalAlreadyRecorded
    );
}

#[tokio::test]
async fn progress_size_is_limited_without_storing_secret_like_payloads() {
    let registry = registry();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let release = std::sync::Arc::new(tokio::sync::Mutex::new(Some(release_rx)));
    let id = registry
        .start(OperationStartRequest::new("connectivity", owner(), {
            let release = std::sync::Arc::clone(&release);
            move |_| {
                Box::pin(async move {
                    let rx = release.lock().await.take().expect("release receiver");
                    let _ = rx.await;
                    OperationTerminal::Completed
                })
            }
        }))
        .expect("operation starts");

    let error = registry
        .progress(id, "x".repeat(33))
        .expect_err("oversized progress is rejected");
    assert_eq!(
        error,
        OperationRegistryError::ProgressTooLarge { limit_bytes: 32 }
    );
    assert!(registry.status(id).unwrap().progress.is_empty());
    release_tx.send(()).expect("release");
    assert_eq!(
        wait_for_terminal(&registry, id).await,
        OperationTerminal::Completed
    );
}

#[tokio::test]
async fn cancellation_pushes_backend_token_and_reports_stopped_or_still_stopping() {
    let registry = registry();
    let id = registry
        .start(OperationStartRequest::new(
            "connectivity",
            owner(),
            |context| {
                Box::pin(async move {
                    context.cancellation_token.cancelled().await;
                    OperationTerminal::Cancelled
                })
            },
        ))
        .expect("operation starts");

    let outcome = registry
        .cancel(id, Duration::from_secs(1))
        .await
        .expect("cancel succeeds");
    assert_eq!(
        outcome,
        OperationCancelOutcome::Stopped {
            terminal: OperationTerminal::Cancelled
        }
    );
    assert_eq!(
        registry
            .cancel(id, Duration::from_millis(1))
            .await
            .expect("second cancel observes terminal"),
        OperationCancelOutcome::AlreadyTerminal {
            terminal: OperationTerminal::Cancelled
        }
    );

    let stubborn = registry
        .start(OperationStartRequest::new("stubborn", owner(), |_| {
            Box::pin(async {
                std::future::pending::<()>().await;
                #[allow(unreachable_code)]
                OperationTerminal::Completed
            })
        }))
        .expect("stubborn operation starts");
    assert_eq!(
        registry
            .cancel(stubborn, Duration::ZERO)
            .await
            .expect("cancel reports still stopping"),
        OperationCancelOutcome::StillStopping
    );
    assert!(matches!(
        registry.status(stubborn).unwrap().state,
        OperationState::Stopping
    ));
    assert_eq!(
        registry
            .cancel(stubborn, Duration::ZERO)
            .await
            .expect("second cancel reports still stopping"),
        OperationCancelOutcome::StillStopping
    );
}

#[tokio::test]
async fn deadline_timeout_and_commit_barrier_result_unknown_are_terminal() {
    let registry = registry();
    let timed_out = registry
        .start(
            OperationStartRequest::new("timeout", owner(), |_| {
                Box::pin(async {
                    std::future::pending::<()>().await;
                    #[allow(unreachable_code)]
                    OperationTerminal::Completed
                })
            })
            .with_deadline(Duration::from_millis(1)),
        )
        .expect("timeout operation starts");
    assert_eq!(
        wait_for_terminal(&registry, timed_out).await,
        OperationTerminal::TimedOut
    );

    let commit = registry
        .start(OperationStartRequest::new(
            "remote-create",
            owner(),
            |context| {
                Box::pin(async move {
                    context.enter_commit_barrier();
                    context.cancellation_token.cancelled().await;
                    OperationTerminal::Cancelled
                })
            },
        ))
        .expect("commit operation starts");
    assert_eq!(
        registry
            .cancel(commit, Duration::from_secs(1))
            .await
            .expect("cancel after commit barrier"),
        OperationCancelOutcome::Stopped {
            terminal: OperationTerminal::ResultUnknown
        }
    );
}

#[tokio::test]
async fn detach_policy_is_fixed_by_operation_kind() {
    let registry = registry();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let release = std::sync::Arc::new(tokio::sync::Mutex::new(Some(release_rx)));
    let detached = registry
        .start(
            OperationStartRequest::new("capture", owner(), {
                let release = std::sync::Arc::clone(&release);
                move |_| {
                    Box::pin(async move {
                        let rx = release.lock().await.take().expect("release receiver");
                        let _ = rx.await;
                        OperationTerminal::Completed
                    })
                }
            })
            .with_cancellation_policy(CancellationPolicy::Detach),
        )
        .expect("detached operation starts");

    assert_eq!(
        registry.detach(detached).expect("detach succeeds"),
        relay_pool_desktop_lib::background_tasks::OperationDetachOutcome::Detached
    );
    assert_eq!(registry.metrics().running, 1);
    release_tx.send(()).expect("release detached");
    assert_eq!(
        wait_for_terminal(&registry, detached).await,
        OperationTerminal::Completed
    );
}

#[tokio::test]
async fn terminal_capacity_and_gc_only_evict_terminal_operations() {
    let registry = OperationRegistry::new(OperationRegistryConfig {
        terminal_ttl: Duration::ZERO,
        terminal_max_entries: 1,
        ..registry_config()
    });
    let first = registry
        .start(OperationStartRequest::new(
            "first",
            owner(),
            immediate(OperationTerminal::Completed),
        ))
        .expect("first starts");
    assert_eq!(
        wait_for_terminal(&registry, first).await,
        OperationTerminal::Completed
    );
    let second = registry
        .start(OperationStartRequest::new(
            "second",
            owner(),
            immediate(OperationTerminal::Completed),
        ))
        .expect("second starts");
    assert_eq!(
        wait_for_terminal(&registry, second).await,
        OperationTerminal::Completed
    );
    assert_eq!(
        registry
            .status(first)
            .expect_err("oldest terminal tombstoned"),
        OperationRegistryError::Expired
    );
    assert!(registry.status(second).is_ok());

    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let release = std::sync::Arc::new(tokio::sync::Mutex::new(Some(release_rx)));
    let running = registry
        .start(OperationStartRequest::new("running", owner(), {
            let release = std::sync::Arc::clone(&release);
            move |_| {
                Box::pin(async move {
                    let rx = release.lock().await.take().expect("release receiver");
                    let _ = rx.await;
                    OperationTerminal::Completed
                })
            }
        }))
        .expect("running starts");
    registry.gc(Instant::now() + Duration::from_secs(1));
    assert!(registry.status(running).is_ok());
    release_tx.send(()).expect("release running");
    assert_eq!(
        wait_for_terminal(&registry, running).await,
        OperationTerminal::Completed
    );
}
