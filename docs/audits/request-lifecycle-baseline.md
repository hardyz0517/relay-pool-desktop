# Request Lifecycle Baseline

日期：2026-07-19
仓库：`D:\Dev\Projects\relay-pool-desktop`
范围：local v2 proxy request admission, candidate attempts, protocol completion, delivery and finalization

## Current Authority Chain

```text
ingress::handle
  -> CanonicalProxyRequest (owns RequestLease)
  -> V2ProxyExecutor::execute
  -> ExecutionEngine::execute
  -> ProxyExecutionResponse (constructs FinalRequestOutcome)
  -> FinalizingStream (mutates outcome after body/protocol events)
  -> FinalizationDispatcher
  -> RoutingRepository::record_final_outcome
  -> AppDatabase
```

## Confirmed Production Symbols

| Concern | Current authority | Evidence |
|---|---|---|
| request/attempt outcome | `FinalRequestOutcome` | `src-tauri/src/services/proxy/routing_repository.rs` |
| candidate feedback | `CandidateFeedback` | `src-tauri/src/services/proxy/routing_repository.rs` |
| protocol mode | `ResponseMode` | `src-tauri/src/services/proxy/endpoint_adapter.rs`, `execution.rs` |
| body finalization | `FinalizingStream` / `FinalizationDispatcher` | `src-tauri/src/services/proxy/response_body.rs` |
| request admission lease | `CanonicalProxyRequest::_request_lease` | `src-tauri/src/services/proxy/request.rs` |
| persistence authority | `AppDatabase` | current crate; no Persistence V2 runtime exists |

## Upgrade Slice Evidence

The following new crate-private modules are now implemented without production wiring:

- `src-tauri/src/services/proxy/lifecycle/`
- `src-tauri/src/services/proxy/protocol/`

Focused evidence:

```text
services::proxy::lifecycle: 7 passed
services::proxy::protocol: 7 passed
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check: passed
cargo check --manifest-path src-tauri/Cargo.toml (isolated CARGO_TARGET_DIR): passed with existing dead-code warnings
```

These results prove only the pure kernel/protocol/writer slice. They do not prove production routing, Persistence V2, stream delivery, SQLite facts or real authenticated E2E.

## Hard Blockers Before Cutover

1. Persistence V2 runtime/store/schema is absent from the repository.
2. Existing production proxy still owns `FinalRequestOutcome`, `CandidateFeedback`, `ResponseMode` and `FinalizationDispatcher`.
3. `RequestLease` has not transferred to a response-body lifecycle owner.

No production SQL adapter may be added to bypass these blockers.
