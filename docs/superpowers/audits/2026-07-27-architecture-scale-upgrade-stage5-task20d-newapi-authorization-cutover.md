# Stage 5 Task 20.D Audit - NewAPI Authorization Cutover

Date: 2026-07-27

## Scope

- Cut over NewAPI authorization validation to the static provider registry and `AsyncOutboundClient`.
- Keep WebView window lifecycle, cookie capture, capture-session commit, endpoint revision barrier and secret persistence in the capture/application owners.
- Remove the old production `ureq` self-probe from `services::capture::web_authorization`; that module now only owns pure cookie/candidate helpers.
- Leave password-login and remaining legacy NewAPI adapter code for later Task 20/22/25/26 shards.
- Persistence V2 remained untouched.

## Implementation Notes

- Added `NewApiAuthorizationDriver` implementing `AuthorizationDriver`.
- Registered NewAPI authorization in `static_provider_entries` with header and session validation support.
- The authorization driver validates `GET /api/user/self` through shared async outbound using a short-lived `DriverSecretAccessor`.
- `CaptureCommandFacade` now injects `AsyncOutboundClient` and `ProviderRegistry` from the composition root and passes only station identity, endpoint, candidate user id and captured cookie into the driver.
- Capture still owns reading cookies from the WebView, committing the captured candidate, persisting the session through `CredentialService`, and writing the collector snapshot.
- Provider matrix and fixtures now mark NewAPI `authorization` as supported with success, partial, auth failure, rate limit, server failure, malformed, unknown shape, cancellation, budget exhaustion, stale revision and redaction scenarios.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib`
- `cargo test --locked --manifest-path src-tauri\Cargo.toml services::collectors::drivers::newapi --lib -- --nocapture` - 16 passed
- `cargo test --locked --manifest-path src-tauri\Cargo.toml newapi_ --lib -- --nocapture` - 18 passed, 1 live test ignored
- `cargo test --locked --manifest-path src-tauri\Cargo.toml web_authorization --lib -- --nocapture` - 12 passed
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

- Live NewAPI web authorization qualification is not included in this deterministic shard and remains a Stage 7 release qualification item.
- Legacy NewAPI password login and remaining `ureq` paths are still scheduled for later Task 20/22/25/26 work; this shard only removes the NewAPI web authorization self-probe fallback.
