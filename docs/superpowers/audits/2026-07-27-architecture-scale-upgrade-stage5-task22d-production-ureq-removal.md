# Stage 5 Task 22.D Audit - Production Ureq Removal

Date: 2026-07-27

## Scope

- Remove the last production `ureq` transport surface from provider and management login/probe paths.
- Move station login probes to the shared `AsyncOutboundClient` contract.
- Keep legacy sync login fixtures test-only while removing `ureq` from the normal dependency graph.
- Remove the stale production HTTP construction allowlist for `services::outbound`.
- Keep Persistence V2 untouched.

## Implementation Notes

- `services::outbound` now only owns neutral proxy config normalization and Windows system proxy URL parsing in production.
- The legacy `credential_agent_builder_for_proxy` remains available only under `cfg(test)` for retained parser/fixture coverage.
- `services::collectors::login_probe` implements async NewAPI and Sub2API login probes using typed `OutboundRequest` values and shared cancellation/correlation inputs.
- `commands::test_station_login_input` and `StationCollectionCommandFacade::test_station_login` now complete network login probes through `AsyncOutboundClient` instead of `BlockingExecutor` plus sync transport.
- NewAPI password login cutover persists successful cookie sessions with `session_source: "password_login"` through the existing collector source port.
- Legacy NewAPI/Sub2API sync HTTP helpers are test-only; production auth context preparation reads existing persisted session state instead of performing sync password login fallback.
- `ureq` moved from Cargo normal dependencies to dev-dependencies, so test fixtures can keep local HTTP coverage without reintroducing production transport.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
- `pnpm generate:bindings --check` - passed, 4 artifacts, two-run deterministic
- `pnpm exec tsc --noEmit`
- `node scripts\architecture\check-command-state-boundaries.mjs` - 104 migrated commands
- `node scripts\architecture\check-typescript-boundaries.mjs` - 939 resolved edges
- `node scripts\architecture\check-command-registry.mjs` - 125 commands
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib` - passed with existing dead-code warnings outside this shard
- `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` - 4 passed
- `cargo tree --locked --manifest-path src-tauri\Cargo.toml -e normal -i ureq` - no normal dependency path printed
- `cargo test --locked --manifest-path src-tauri\Cargo.toml login_probe --lib -- --nocapture` - 2 passed
- Persistence V2 zero diff: `git diff -- src-tauri/src/persistence src-tauri/migrations docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Known Follow-Up

- Remaining legacy parser/test fixture code is intentionally retained under `cfg(test)` and should be deleted only when its fixture coverage has a replacement.
- Cargo warning cleanup for pre-existing Persistence V2, credentials and request recovery dead-code surfaces remains outside this Task 22 shard.
- This shard closes Task 22's production `ureq` removal requirement, but Stage 5 Gate still depends on the broader provider capability/conformance closeout.
