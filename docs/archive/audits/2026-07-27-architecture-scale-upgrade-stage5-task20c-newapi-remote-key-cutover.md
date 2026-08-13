# Stage 5 Task 20.C Audit - NewAPI Remote-Key Cutover

Date: 2026-07-27

## Scope

- Cut over NewAPI remote-key list/create/reveal production paths to the static provider registry and `AsyncOutboundClient`.
- Preserve the existing application-owned local persistence flow: driver returns remote-key facts and one-time secret; `services::remote_keys` still performs enrichment, endpoint revision checks, group binding selection and `CredentialService` persistence.
- Leave NewAPI authorization/WebView capture behavior unchanged for Task 20.D.
- Leave Sub2API remote-key legacy path unchanged for Task 21.
- Persistence V2 remained untouched.

## Implementation Notes

- Extended `RemoteKeyDriver` with list, reveal and create operations plus canonical remote-key outputs and a non-`Debug` `RemoteKeySecret`.
- Registered `NewApiRemoteKeyDriver` in `static_provider_entries` and marked NewAPI `remote_key` as supported in the provider matrix.
- NewAPI remote-key driver uses shared async outbound for:
  - `GET /api/token/?p={page}&page_size=100`
  - `POST /api/token/{token_id}/key`
  - `POST /api/token/`
- Create requests set `OutboundRetryPolicy::Never`; lost/transport-unknown create outcomes map to typed `ResultUnknown` so callers must reconcile before retrying.
- NewAPI string dispatch in the legacy remote-key service path now fails closed with `async capability driver required` messages if accidentally reached.
- Existing legacy NewAPI adapter helpers remain physically present because Task 20.D authorization still depends on the old NewAPI adapter module.

## Deterministic Evidence

- `cargo fmt --manifest-path src-tauri\Cargo.toml --check`
- `cargo check --locked --manifest-path src-tauri\Cargo.toml --lib`
- `cargo test --locked --manifest-path src-tauri\Cargo.toml services::collectors::drivers::newapi --lib -- --nocapture` - 14 passed
- `cargo test --locked --manifest-path src-tauri\Cargo.toml newapi_ --lib -- --nocapture` - 19 passed, 1 live test ignored
- `cargo test --locked --manifest-path src-tauri\Cargo.toml remote_key --lib -- --nocapture` - 15 passed
- `cargo test --locked --manifest-path src-tauri\Cargo.toml --test provider_conformance -- --nocapture` - 29 passed
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
- Persistence V2 zero diff: `git diff -- src-tauri/src/persistence src-tauri/migrations docs/audits/persistence-v2-boundary-manifest.json`

## Known Follow-Up

- `cargo check/test` still reports dead-code warnings in the legacy NewAPI adapter. This is expected after collector and remote-key cutover because authorization remains scheduled for Task 20.D and physical deletion remains Task 25/26 work.
- NewAPI remote-key live qualification is not included in this deterministic shard and remains a Stage 7 release qualification item.
