# Request Lifecycle Deletion Ledger

Status: Task 17 post-cutover architecture gate; Task 18 release validation has current release-exe smoke evidence and remains blocked only at the real signed-installer gate.

Task 18 snapshot (2026-07-20):

- Passed: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- Passed after current cleanup: `node scripts/request-lifecycle-architecture.test.mjs`
- Passed after current cleanup: `pnpm.cmd test:contracts`
- Passed: `pnpm.cmd test`
- Passed on current tree: `pnpm.cmd build`
- Passed after current cleanup: `cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy` (253 tests)
- Passed after current cleanup: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_domain -- --nocapture` (8 tests)
- Passed after current cleanup: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_protocol_contracts -- --nocapture` (10 tests)
- Passed after current cleanup: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_persistence -- --nocapture` (1 test)
- Passed after current cleanup: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_faults -- --nocapture` (11 tests; includes expected injected-panic output)
- Passed after current cleanup: `cargo test --manifest-path src-tauri/Cargo.toml --test proxy_lifecycle_concurrency -- --nocapture` (8 tests)
- Passed on current tree: `cargo check --manifest-path src-tauri/Cargo.toml` with `CARGO_TARGET_DIR=output\task18-current-check-target`.
- Passed on current tree: `cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::startup_auto_start -- --nocapture`.
- Passed on current tree: `cargo test --manifest-path src-tauri/Cargo.toml --lib local_proxy_start_on_launch_defaults_false_and_persists -- --nocapture`.
- Passed on current tree: `cargo test --manifest-path src-tauri/Cargo.toml --lib models_aggregation_preserves_attempt_count_in_lifecycle_evidence -- --nocapture`.
- Passed on current tree after downstream-cancel fix: `cargo test --manifest-path src-tauri/Cargo.toml --lib services::proxy::response_body -- --nocapture` (8 tests).
- Passed after current cleanup: `powershell -ExecutionPolicy Bypass -File scripts\run-proxy-lifecycle-soak.ps1 -Smoke` (1 pass; resource counters returned to zero; p95 9831.52 ms)
- Passed: `cargo build --manifest-path src-tauri/Cargo.toml --bins --features tauri/custom-protocol --release -vv` with `CARGO_TARGET_DIR=output\task18-release-cargo-target`.
- Passed on current tree: `cargo build --manifest-path src-tauri/Cargo.toml --bins --features tauri/custom-protocol --release` with `CARGO_TARGET_DIR=output\task18-current-release-target`.
- Passed on current tree: release exe persisted-start smoke. The test temporarily set `local_proxy_start_on_launch=true`, launched `output\task18-current-release-target\release\relay-pool-desktop.exe` without Vite/dev server, observed `127.0.0.1:8787` listening, and ran `scripts\verify-local-routing-lifecycle.ps1 -Smoke` against the real app database. Verified request ids: `req_0000019f7e0b6614_00008b74_0000000000000001` (`/v1/models`, HTTP 200, SQLite `attempt_count=2`, `fallback_count=1`, no legacy `attempts_json`) and `req_0000019f7e0b6614_00008b74_0000000000000002` (`/v1/chat/completions`, HTTP 200, SQLite `attempt_count=1`). The test restored the prior `local_proxy_start_on_launch=false` setting and stopped the launched process.
- Found and fixed during current live matrix: `chat-stream-cancel` produced `request_logs.attempt_count=1` but `request_attempts` had zero rows for `req_0000019f7e138c36_000090e8_0000000000000006`. Root cause was `LifecycleBody::finalize_downstream_drop` finalizing the request with `DownstreamDropped` while passing no selected-attempt terminal. The fix records a neutral `AttemptFailureKind::DownstreamDrop` terminal before request finalization.
- Live matrix status after the fix: chat/responses stream paths previously reached SQLite verification, but the latest release-exe rerun is currently blocked by real upstream/config state before it reaches cancel verification. Evidence: `/v1/embeddings` with `gpt-5.4-mini` failed as `route_no_candidate` because the real DB has no embedding-capable key (`station_key_capabilities.supports_embeddings=0`), and a later rerun failed on `/v1/models`/chat with upstream auth/route errors (`req_0000019f7e1ddc42_00005fb8_0000000000000001` upstream HTTP 403 on `key-1784427389639-145`, followed by `req_0000019f7e1ddc42_00005fb8_0000000000000002` route_no_candidate). These are not counted as successful full live matrix gates.
- Passed with unsigned artifact produced / blocked at signing: `pnpm.cmd exec tauri build --verbose` compiled the release binary and produced the NSIS setup executable, then exited 1 because a Tauri public key is configured but `TAURI_SIGNING_PRIVATE_KEY` is not present in the environment. This is not counted as a fully passed release gate until the signed build exits 0 with the real signing key.
- Not run in this shared busy workspace: formal 60-minute soak. The smoke pass is not a substitute for the formal soak if this build is promoted to release.

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
| debug-only proxy auto-start | production `startup_auto_start` plus persisted `local_proxy_start_on_launch` intent | `dev_auto_start.rs` is deleted. App setup always schedules the production startup coordinator; installed/release builds no longer depend on `debug_assertions`. |
| downstream-drop request-only terminal | `LifecycleBody` selected-attempt terminal plus request terminal | Downstream disconnect now writes a neutral `DownstreamDrop` attempt terminal before `DownstreamDropped` request finalization, keeping `request_logs.attempt_count` and normalized `request_attempts` cardinality aligned. |

Deletion rule: legacy symbols may remain only in historical audit prose or `#[cfg(test)]` legacy fixtures. Production code must pass the architecture fitness gate, Rust lifecycle tests, and SQLite live E2E verification before release.
