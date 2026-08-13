# Stage 5 Task 21.B Audit - Sub2API Remote-Key Cutover

Date: 2026-07-27

## Scope

- Cut over Sub2API remote-key list/create/reveal to the static provider registry and `AsyncOutboundClient`.
- Keep Sub2API collector driver behavior from Task 21.A unchanged.
- Keep Sub2API authorization unsupported.
- Keep Persistence V2 untouched.
- Do not perform Task 22 management/probe HTTP cleanup or global `ureq` dependency removal.

## Implementation Notes

- Added `Sub2ApiRemoteKeyDriver` under `src-tauri/src/services/collectors/drivers/sub2api`.
- Registered Sub2API remote-key capability in `static_provider_entries` with list/create/reveal and result-unknown reconciliation support.
- Extended the capability request contract with a generic `provider_group_id` field so Sub2API create can post the selected provider group id without leaking provider-specific orchestration into the common service.
- Added Sub2API remote-key prepared context in `src-tauri/src/services/remote_keys.rs`; it prepares station identity, endpoint revision, session token/login-password handles, proxy policy and group id, then lets the async driver own HTTP.
- Wired `RemoteKeysCommandFacade` scan/create/reveal to try Sub2API driver context after NewAPI and before the temporary legacy fallback.
- Reused existing Sub2API pure remote-key parsers for canonical key identity, full-key detection, mask/fingerprint behavior and create fallback shape.
- Changed the temporary legacy remote-key string dispatcher so Sub2API scan/create/reveal return explicit `async capability driver` errors instead of calling old synchronous adapter HTTP.
- Updated provider matrix and fixtures so Sub2API remote-key has success, partial, auth failure, rate limit, server failure, malformed, unknown shape, cancellation, budget exhaustion, stale revision and redaction scenarios.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml`
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
- JSON parse: `provider-capability-matrix.json` and provider fixture manifest
- Legacy path search: no production call remains to `adapters::sub2api::{scan_remote_keys, scan_remote_key_full_secret, create_remote_key}`; the temporary fallback now fails closed for Sub2API remote-key operations.
- `git diff --check`
- Persistence V2 zero diff: `git diff -- src-tauri/src/persistence src-tauri/migrations docs/audits/persistence-v2-boundary-manifest.json`

## Known Follow-Up

- Task 22 still owns endpoint ping, channel probe, web authorization validation, updater direct HTTP and final production `ureq` removal.
- Task 25/26 still own broad legacy adapter deletion and final qualification cleanup after all Stage 5 provider/probe cutovers are complete.
- Live Sub2API qualification remains separated from deterministic fixture conformance and belongs to the later qualification stage.
- Stage 5 Gate is not claimed by this shard.
