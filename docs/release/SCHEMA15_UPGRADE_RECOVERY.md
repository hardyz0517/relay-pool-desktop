# Schema 15 Upgrade And Recovery Release Gate

This release gate applies to any Relay Pool Desktop release that changes local SQLite startup, schema migrations, encrypted-secret baseline conversion, device-key handling, or data recovery routing.

Status: historical release gate; current schema15 readiness is governed by [`../plans/2026-09-01-schema15-upgrade-reliability.md`](../plans/2026-09-01-schema15-upgrade-reliability.md). This document is not a release approval until the plan's fixture, restart, maintenance-resume, and install-matrix gates pass on the final tree.

The 2026-07-31 debt manifest is historical evidence and contains stale test names; it must not be used as proof that the current schema `71` route is qualified. A future published build must provide `TAURI_SIGNING_PRIVATE_KEY` or `TAURI_SIGNING_PRIVATE_KEY_PATH` and complete `pnpm verify:release` on the final tree. The current upgrade reliability gaps and their executable acceptance evidence are tracked in the linked plan.

## Declared Compatibility Window

| Item | Release Value |
|---|---|
| Minimum automatic upgrade baseline | schema `15` |
| Latest compatibility schema | schema `71` |
| Latest SQL migration ledger | schema `71` |
| Latest secret format | `1` |
| Fresh-install key behavior | create one active device key before publishing the first generation-2 database |
| Existing-database key behavior | missing or wrong key enters typed recovery; never create a replacement key automatically |

Databases below schema `15` are outside the automatic upgrade window. They must enter `unsupportedSchemaVersion` recovery with backup/export guidance instead of guessed repair.

## Required Startup Route

Every supported generation-2 startup must follow:

```text
read-only probe -> pure plan -> ordered execution -> final verification -> ready or typed recovery
```

The current supported routes are:

```text
schema 15, legacy secrets
  -> structural migration to schema 16
  -> encrypted-secret baseline conversion
  -> schema 17 + secret_format_version 1
  -> structural migrations to schema 71
  -> durable maintenance (including request-log sanitizer) to complete
  -> final verification

schema 16, legacy secrets
  -> encrypted-secret baseline conversion
  -> schema 17 + secret_format_version 1
  -> structural migrations to schema 71
  -> durable maintenance to complete
  -> final verification

schema 17-70, secret_format_version 1
  -> structural migrations to schema 71
  -> durable maintenance to complete
  -> final verification

schema 71, secret_format_version 1
  -> final verification

schema 71 with incomplete durable maintenance or a valid resumable journal
  -> resume the maintenance step before opening runtime
  -> final verification
```

Normal runtime may be registered only after final writable health and secret decryptability checks pass.

## Recovery Behavior

Known startup upgrade failures must map to typed recovery reasons:

| Failure | Recovery Reason |
|---|---|
| schema `< 15` | `unsupportedSchemaVersion` |
| SQL/compatibility/secret-format metadata mismatch | `inconsistentSchemaMetadata` |
| missing existing device key | `missingKey` |
| key exists but cannot decrypt stored secrets | `keyMismatch` |
| SQLite cannot be read or integrity checked | `corruptedDatabase` |
| journaled upgrade is incomplete or invalid | `interruptedUpgrade` |
| structural schema migration fails | `schemaMigrationFailed` |
| encrypted-secret baseline conversion fails before a durable journal exists | `secretBaselineFailed` |
| unexpected upgrade-internal failure | `internalUpgradeError` |

Do not route these through string matching or a generic open/migration bucket.

## Release Evidence

Before publishing a release that changes this area, record evidence for:

- automated Rust coverage that copies the frozen schema `15` fixture and executes the production route through schema `15 -> 71`;
- automated Rust coverage proving schema `15` reaches structural schema `16` before encrypted-secret baseline conversion;
- automated Rust coverage for schema `16 -> latest` and schema `17` idempotent startup;
- missing-key and wrong-key recovery behavior for an existing encrypted database;
- interrupted baseline conversion recovery behavior;
- typed journal-kind routing for generation upgrade versus encrypted-secret baseline conversion, including valid journal resume and cleanup;
- restart coverage for an interrupted request-log sanitizer after the SQL ledger already reaches schema `71`;
- frontend recovery enum/binding regeneration when recovery reasons change;
- `cargo check --manifest-path src-tauri/Cargo.toml`;
- applicable focused Rust tests;
- applicable frontend TypeScript/Vite build checks.
- full `persistence_architecture` in single-thread mode for the production architecture boundary.

## Future Schema Or Secret Format Changes

A future schema release should add one ordered schema step, one postcondition, fixture coverage from schema `15`, a restart assertion, and an updated release note declaring the latest schema.

A future secret-format release should add one typed secret-format step, key precondition tests, secret decryptability verification, interruption handling, and an updated release note declaring the latest secret format.

Do not add per-version startup branches. If a new release needs to change startup orchestration, the schema15 upgrade design must be updated first with the reason and the acceptance evidence.
