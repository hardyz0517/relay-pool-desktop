# Request Lifecycle Deletion Ledger

Status: Task 17 post-cutover architecture gate; Task 18 command-gate verification in progress.

Task 18 snapshot (2026-07-20):

- Passed: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- Passed after current cleanup: `node scripts/request-lifecycle-architecture.test.mjs`
- Previously passed before current cleanup: `pnpm.cmd test:contracts`; post-cleanup aggregate rerun is pending because `scripts/data-store-upgrade-matrix.test.mjs` hit a concurrent Cargo artifact lock while spawning its Rust fixture.
- Passed: `pnpm.cmd test`
- Passed: `pnpm.cmd build`
- Previously passed before current cleanup: `cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy -- --nocapture`; post-cleanup revalidation pending.
- Pending revalidation: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_domain -- --nocapture`
- Pending revalidation: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_protocol_contracts -- --nocapture`
- Pending revalidation: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_persistence -- --nocapture`
- Pending revalidation: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_faults -- --nocapture`
- Pending revalidation: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_concurrency -- --nocapture`
- Passed: `cargo check --manifest-path src-tauri/Cargo.toml`
- Pending/blocked: the current workspace also has a concurrent `cargo build --manifest-path src-tauri/Cargo.toml --bins --features tauri/custom-protocol --release` process. Request-lifecycle Rust revalidation, integration wrappers, aggregate contract rerun, and `pnpm.cmd tauri:build` are not counted as passed until they exit 0 from a clean, non-overlapping verification run.

| Legacy symbol/path | Replacement | Task 17 result |
|---|---|---|
| `FinalRequestOutcome` | `FinalRequestRecord` from `RequestLifecycle` | Production symbol absent; guarded by `scripts/request-lifecycle-architecture.test.mjs`. |
| `CandidateFeedback` | `AttemptTerminalRecord.health` | Production symbol absent; health effect is carried by normalized attempt terminal records. |
| `FailedRequestContext` | `ClassifiedAttemptFailure` + request terminal record | Production symbol absent. |
| `ResponseMode` | `ResponsePlan` | Production symbol absent; protocol plan is compiled by endpoint adapter/protocol contract. |
| `ProxyHttpResponse.outcome` | response status/headers/payload plus lifecycle admission/finalization lease | Struct contains only `status`, `headers`, and `payload`; no pseudo outcome field. |
| `FinalizingStream` | `LifecycleBody` | Renamed and reduced to delivery/protocol body wrapper; no DB store import or old mutable outcome. |
| `FinalizationDispatcher` | ordered `LifecycleWriter` | Production symbol absent; writes go through one bounded channel. |
| `RequestLease` in `CanonicalProxyRequest` | lease transferred into `LifecycleBody` until body terminal/drop/cancel | Request still carries the lease only as an ingress-to-body handoff; body owns terminal release. |
| `attempts_json` write path | normalized `request_attempts` | Production writes are stopped; column remains schema/read compatibility only for one release cycle. `ProxyFailure.attempts_json` was removed so failures cannot become a second projection carrier. |
| `AttemptTrace` | `AttemptTerminalRecord` / normalized `request_attempts` | Removed after cutover; architecture gate now rejects the symbol in production proxy code. |
| `legacy_runtime` production export | `runtime.rs` V2 `ProxyRuntimeState` | `legacy_runtime.rs` remains test-only (`#[cfg(test)]`) and is not production-exported. |
| `AppDatabase` proxy finalization methods | Persistence V2 consumer-owned port | Runtime composes `RequestLifecyclePersistenceService` and `RequestLogStore`; execution/response body do not construct legacy request-log DTOs. |

Deletion rule: legacy symbols may remain only in historical audit prose or `#[cfg(test)]` legacy fixtures. Production code must pass the architecture fitness gate, Rust lifecycle tests, and SQLite live E2E verification before release.
