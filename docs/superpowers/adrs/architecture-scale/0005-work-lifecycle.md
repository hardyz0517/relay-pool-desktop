# ADR 0005: Structured work lifecycle

## Status

Accepted. `TaskTracker` is the sole primitive for registry-level join ownership. `TaskSupervisor` and `OperationRegistry` keep separate scoped tracker instances and registries. A bounded local `JoinSet` is allowed only inside one task/operation for short fan-out and may not outlive that function or register application lifecycle state.

## Context

Current runners use threads, atomics, ad-hoc blocking work and late shutdown in `RunEvent::Exit`. Tray/window paths call `app.exit(0)` directly. Long foreground actions often have only frontend run tokens. This cannot guarantee admission, cancellation, terminal state or bounded shutdown.

## Decision

Reuse Tokio/Tokio-util primitives rather than build a runtime:

- `CancellationToken` carries hierarchical cancellation;
- `tokio_util::task::TaskTracker` owns application task admission, close and wait;
- `Semaphore` and bounded `mpsc` enforce admission/backpressure;
- `watch` publishes latest state where lossless history is not required.

`TaskTracker` is selected over a registry-level `JoinSet` because tasks need cloneable spawn ownership across composed runners and a separate close-then-wait lifecycle. A task wrapper catches and records join/panic outcome as terminal diagnostics. Closing admission and spawning must be serialized by the supervisor state machine; after `Stopping`, new admission returns typed `ShuttingDown`. Repeated shutdown returns the same report and does not spawn another drain.

`TaskSupervisor` owns recurring daemon metadata, cancellation, restart/backoff and status only. `OperationRegistry` separately owns user-triggered long operations. Admission atomically allocates id, checks global/per-key capacity, registers the handle and spawns it. Progress uses a 128-entry ring; terminal summaries live 15 minutes, at most 256 entries, and GC runs every 60 seconds. Running operations are never evicted. After GC, lookup returns typed `Expired` rather than ambiguous `NotFound` when a tombstone is still retained.

`BlockingExecutor` has 4 concurrent permits, a 16-item queue and a 2-second queue wait. Capacity failure is typed `Overloaded`. It is only for genuinely blocking calls, not synchronous HTTP that has an async client. Detached/orphaned work is diagnosed and cannot hold application shutdown indefinitely.

All exit sources enter one idempotent `ExitCoordinator`. At Tauri `ExitRequested`, it prevents immediate exit, closes admission, cancels children, performs bounded asynchronous drain and then requests final exit exactly once. `RunEvent::Exit` performs no principal `block_on` shutdown. The global deadline is 45 seconds; per-kind limits and remaining-budget propagation are fixed in the budget ledger. A child never receives a fresh full timeout.

Custom executors, thread pools, generic mailboxes, actor addresses, workflow DSLs and unowned `spawn` are forbidden.

## Alternatives

- Registry-level `JoinSet` as primary owner: rejected because distributed registrants would require a central mutable poll/ownership loop and duplicate registry policy.
- Actor framework or workflow engine: rejected because current work is lifecycle coordination, not a distributed message system.
- Unlimited `spawn_blocking`: rejected because saturation becomes invisible and shutdown cannot be bounded.
- Shutdown only in `RunEvent::Exit`: rejected because async drain begins after the preventable lifecycle phase.

## Consequences

Admission may reject work under load, which is intentional and observable. Task bodies must cooperate with cancellation and budget propagation. Panic, timeout and orphan state become reportable. Supervisor policy stays small because Tokio remains the executor.

## Rollback

Each runner/operation migrates independently. Rollback restores its previous registration and body while leaving the shared supervisor intact. Once an exit source uses `ExitCoordinator`, it cannot return to direct `app.exit` without reopening this ADR and the shutdown fault matrix.

## Verification

- state-machine tests cover panic/join error, close/wait, admission race, cancellation before/after spawn and repeated shutdown;
- saturation tests prove exact queue/concurrency limits and typed overload;
- operation tests prove one terminal state, progress lag recovery, terminal TTL/capacity and running-handle non-eviction;
- shutdown tests cover tray, window, updater and OS exit, enforce 45 seconds and report every timed-out/orphaned owner;
- architecture gates reject custom runtime constructs and application-level `JoinSet` ownership outside the approved local fan-out helper boundary;
- Tauri lifecycle tests prove final exit is requested once and principal shutdown does not run in `RunEvent::Exit`.
