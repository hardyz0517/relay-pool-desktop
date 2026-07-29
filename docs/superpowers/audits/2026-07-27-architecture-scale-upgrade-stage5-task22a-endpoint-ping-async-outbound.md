# Stage 5 Task 22.A Audit - Endpoint Ping Async Outbound Cutover

Date: 2026-07-27

## Scope

- Cut over station endpoint ping from direct synchronous `ureq` construction to the shared `AsyncOutboundClient`.
- Remove the endpoint-ping `spawn_blocking` facade boundary and endpoint-ping `ureq::AgentBuilder` allowlist entries.
- Preserve the existing HEAD-first, GET-fallback probe semantics and health persistence behavior.
- Keep channel probe, web authorization validation, updater inspection and final `ureq` dependency cleanup for later Task 22 shards.
- Keep Persistence V2 untouched.

## Implementation Notes

- `services::endpoint_ping::ping_station_endpoint` is now async and accepts the shared outbound client plus a cancellation token.
- Endpoint ping builds an `OutboundRequest` with direct proxy policy, no retries, a request budget derived from the caller timeout and a minimal public `Accept: */*` header.
- `RoutingCommandFacade` now owns `AsyncOutboundClient` and awaits endpoint ping directly instead of entering `tokio::task::spawn_blocking`.
- The app composition root injects the existing shared outbound client into the routing command facade.
- Endpoint ping unit tests now use the architecture outbound client config and async Tokio tests.
- The architecture boundary manifest removed the expired Task 22 endpoint-ping exceptions for direct facade blocking and direct `ureq` agent construction.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo test --locked --manifest-path src-tauri\Cargo.toml endpoint_ping --lib -- --nocapture` - 6 passed
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib`
- `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` - 4 passed
- JSON parse: `architecture-scale-boundary-manifest.json`
- Persistence V2 zero diff: `git diff -- src-tauri/src/persistence src-tauri/migrations docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Known Follow-Up

- Task 22 still owns channel probe, web authorization HTTP validation, updater direct HTTP inspection and final production `ureq` removal.
- Stage 5 Gate is not claimed by this shard.
