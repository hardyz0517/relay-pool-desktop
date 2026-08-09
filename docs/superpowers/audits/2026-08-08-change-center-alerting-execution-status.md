# Change Center Alerting Execution Status

Date: 2026-08-08
Plan: `docs/superpowers/plans/2026-08-08-change-center-alerting-upgrade.md`

This file records implementation evidence only. It does not replace the
approved specification or the execution plan.

## Verified

- Alerting domain model, event registry, lifecycle reducer, policy resolver,
  delivery ledger, retention worker, and supervised runtime task exist.
- Migration `0029_change_center_alerting_foundation.sql` creates the durable
  occurrence, incident, attention, policy, delivery, and upgrade-progress
  schema. Persistence contract test passed.
- Startup upgrade planning and execution run the durable history backfill and
  current-facts rebuild before `OpenRuntime`; failures map to typed recovery.
- Collector producer paths use `AlertingIngress::record_in_session` in the
  authoritative fact transaction. Legacy producer helpers were removed.
- Alerting IPC commands, explicit DTOs, ACL/registry entries, generated
  bindings, settings entry, Change Center current read model, and AppShell
  aggregate badge are wired.
- Change Center now exposes cursor-backed incident detail, occurrence history,
  and delivery history read models through dedicated IPC commands and renders
  bounded history panels without loading raw payloads.
- History command inputs enforce bounded episode identifiers and page limits at
  the Rust DTO boundary; generated bindings are regenerated deterministically.
- The test-notification IPC command is explicit: in-app test succeeds and the
  desktop channel returns a bounded unavailable-runtime error until the native
  adapter is installed.
- Verified commands:
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- `cargo check --locked --manifest-path src-tauri/Cargo.toml`
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --nocapture`
  (`819 passed; 0 failed`)
- `cargo test --locked --manifest-path src-tauri/Cargo.toml --tests`
  (library and all integration test binaries passed)
- persistence architecture gate (`42 passed; 0 failed`), including the new
  alerting settings, retention, and cursor read-model store boundaries
- generation upgrade matrix (`12 passed; 0 failed`), including schema 15/16/17,
  wrong-key, restart and rollback paths
- `pnpm.cmd build`
- `pnpm.cmd test:contracts`
- `pnpm.cmd architecture:scale-baseline`
- `pnpm.cmd generate:bindings`
  - `pnpm.cmd generate:bindings --check`
  - `pnpm.cmd architecture:commands`
  - `pnpm.cmd architecture:alerting`
- `pnpm.cmd architecture:security`
  - alerting persistence, alerting upgrade, startup upgrade, and frontend
    alerting/settings/change tests
- The routing security source contract was updated to assert the current
  `enforce_latest_maintenance && sqlx_version >= 18` runtime gate.
- The persistence V2 boundary manifest now records the alerting graph and no
  longer allowlists deleted change-center modules.

## Not Yet Qualified

- Task 12 production cutover is recorded in
  `change-center-alerting-cutover-manifest.json`; legacy IPC, view models,
  query keys, and production readers have been removed.
- The native desktop notification adapter and permission state are wired, but
  Windows click-to-deep-link behavior remains best-effort because the official
  Tauri notification API has no reliable desktop click callback.
- Observation-period zero-reader evidence and destructive legacy-table
  migration have not run.
- Full verification (`verify:fast`, `verify:full`) and Windows notification
  smoke remain release gates after Tasks 12-14. `verify:fast` passes; the most
  recent `verify:full` reached the advisory stage and was blocked because the
  configured RustSec database could not resolve `github.com`.
- Qualification and acceptance evidence are recorded in
  `change-center-alerting-qualification.md` and
  `change-center-alerting-acceptance-matrix.md`; observation-period and backup/
  restore rows remain open.

## Gate

Task 12 is complete. Do not delete the retained historical table paths or mark
the upgrade fully complete until the observation, destructive migration, and
release gates in Tasks 13-14 pass.
