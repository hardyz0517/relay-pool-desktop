# Stage 5 Task 21.A Audit - Sub2API Collector Cutover

Date: 2026-07-27

## Scope

- Cut over Sub2API collector capability to the static provider registry and `AsyncOutboundClient`.
- Keep Sub2API remote-key list/create/reveal on the legacy adapter path for Task 21.B.
- Keep Persistence V2 untouched.
- Preserve existing Sub2API canonical facts for usage balance, group/rate parsing, account profile fallback, dashboard usage stats and single-group key binding.
- Remove the production collector string dispatcher fallback for Sub2API; collector commands must enter through the capability driver route.

## Implementation Notes

- Added `Sub2ApiCollectorDriver` under `src-tauri/src/services/collectors/drivers/sub2api`.
- Registered Sub2API collector support in `static_provider_entries`; remote-key and authorization remain unsupported in the registry.
- Added Sub2API auth metadata to `ProviderAuthContext` and a multi-secret accessor for station-key, session-token and login-password handles.
- `prepare_sub2api_collection_v2` reads station, key, session and login inputs, then the async finish phase executes provider HTTP through `AsyncOutboundClient`.
- Sub2API driver supports `Detect`, `Balance` and `Groups`; `Models` and `Full` remain explicit unsupported driver tasks, with Full still split by the collector parent task.
- Reused old adapter pure parsers/mergers for baseline fact compatibility; old `ureq` collector HTTP functions are no longer reachable from the collector dispatcher.
- Provider matrix and fixtures now mark Sub2API collector as supported with success, partial, auth failure, rate limit, server failure, malformed, unknown shape, cancellation, budget exhaustion, stale revision and redaction scenarios.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib`
- `cargo test --locked --manifest-path src-tauri\Cargo.toml sub2api --lib -- --nocapture` - 31 passed
- `cargo test --locked --manifest-path src-tauri\Cargo.toml --test provider_conformance -- --nocapture` - 31 passed
- `cargo test --locked --manifest-path src-tauri\Cargo.toml --test architecture_scale_boundaries -- --nocapture` - 4 passed
- `pnpm generate:bindings --check`
- `pnpm exec tsc --noEmit`
- `node scripts\architecture\check-command-state-boundaries.mjs`
- `node scripts\architecture\check-typescript-boundaries.mjs`
- `node scripts\architecture\check-build-entries.mjs`
- `node scripts\architecture\check-command-registry.mjs`
- `node scripts\architecture\check-tauri-security.mjs`
- `node scripts\architecture\check-artifact-policy.mjs`
- `node scripts\architecture\check-dependency-lifecycle.mjs`
- `node scripts\architecture\check-fixtures.mjs`
- JSON parse: `provider-capability-matrix.json`, provider fixture manifest and architecture boundary manifest
- Persistence V2 zero diff: `git diff -- src-tauri/src/persistence src-tauri/migrations docs/superpowers/audits/persistence-v2-boundary-manifest.json`

## Known Follow-Up

- Task 21.B must cut over Sub2API remote-key capability separately and then remove its legacy provider-name matching path.
- Task 22 still owns remaining production `ureq` cleanup outside this collector cutover.
- Live Sub2API qualification remains separated from deterministic fixture conformance and belongs to the later qualification stage.
