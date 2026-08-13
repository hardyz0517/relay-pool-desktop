# Change Center Alerting Qualification

Date: 2026-08-08
Plan: `docs/plans/2026-08-08-change-center-alerting-upgrade.md`

## Scope

This report qualifies the implementation and recovery paths that can be
verified locally. It does not claim release completion while the observation
period, Windows notification smoke, and external advisory scan remain open.

## Passed evidence

- Rust library tests: `819 passed; 0 failed`.
- Generation upgrade matrix: `12 passed; 0 failed`, including schema 15/16/17,
  released v0.3.1, wrong-key, journal restart, and import rollback paths.
- Persistence alerting integration and portable migration targeted tests pass.
- `pnpm.cmd verify:fast` passes.
- `pnpm.cmd test:contracts` passes, including the alerting architecture gate.
- `pnpm.cmd architecture:scale-baseline` passes against the new alerting
  current-incidents query.
- `pnpm.cmd generate:bindings --check`, `pnpm.cmd build`,
  `pnpm.cmd verify:persistence-artifacts`, `cargo fmt -- --check`, and
  `cargo check --locked` pass.
- The schema 21 migration regression now verifies quarantine before schema 30
  and verifies the postcondition that the legacy table is absent afterward.

## Open qualification items

| Item | Status | Required evidence |
|---|---|---|
| Release observation period | Pending | Stable writer/read-model metrics and zero upgrade-adapter invocations for one release cycle |
| Backup and restore drill | Pending | Verified backup, restore, integrity check, and application restart on a disposable fixture |
| Windows desktop notification smoke | Pending | Allowed, denied, unavailable, and shutdown/crash-boundary cases on Windows |
| RustSec advisory scan | Blocked by environment | Re-run `pnpm.cmd verify:full` with access to the configured advisory database |
| Legacy adapter/catalog removal | Deferred | Only after the three preceding gates are signed off |

## Release decision

The implementation is technically qualified for continued observation and
staging validation. It is not qualified for a final release declaration until
all open items are closed and the deletion ledger is updated accordingly.
