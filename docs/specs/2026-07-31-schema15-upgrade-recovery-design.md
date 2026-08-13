# Schema 15 Baseline Upgrade And Recovery Design

Status: Main route implemented; D-01 through D-09 closed; source-qualified and production-architecture-qualified

Date: 2026-07-31

Scope: Relay Pool Desktop local SQLite startup, schema upgrade, encrypted secret baseline migration, key mismatch handling, and data recovery routing.

## 2026-07-31 Cleanup Debt Gate

The schema `15` upgrade route has a working main line, and the machine-readable debt manifest at `docs/audits/2026-07-31-schema15-upgrade-debt-manifest.json` marks D-01 through D-09 as `closed` with automated evidence. Production architecture qualification is also covered by the `persistence_architecture` release AST gate.

Current gate state:

- Main route evidence exists for schema `15/16/17`, wrong-key recovery, and journal-kind routing.
- D-01 through D-09 are closed.
- Production `application::* -> sqlx::*` forbidden edges are absent according to `cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture --test-threads=1`.
- Publishing a release from this area still requires `pnpm verify:release` on the final tree. GitHub/RustSec access must run through the local PowerShell proxy, and Tauri release bundling requires signing credentials.

## 2026-07-31 Plan Recheck

Verdict: the direction is mature enough to keep, but only under a strict route contract. The mature part is not "schema 15 is special"; the mature part is that schema `15` becomes the declared automatic-upgrade baseline and all later upgrades must reuse the same route:

```text
read-only probe -> pure plan -> ordered execution -> verification -> ready or typed recovery
```

The plan would become immature if future releases add independent startup branches for each version. A release may add a migration step, but it must not add a new startup policy path.

Current first slice status:

- Implemented: bounded SQL migration to a target schema, read-only startup probe, pure startup planner, ordered step execution, explicit `EnsureLatestSchema` future-version step, schema `15/16/17` route tests, typed journal-kind routing, typed recovery reasons, IPC/frontend enum propagation, typed secret validation for key mismatch, explicit secret-format startup metadata, and release gate documentation.
- Verified: schema `15` reaches structural schema `16` before encrypted-secret baseline conversion, schema `16` runs baseline before runtime open, schema `17` opens idempotently, missing key does not become silent key creation, wrong key maps to `keyMismatch`, invalid/interrupted journal maps to `interruptedUpgrade`, generated bindings are deterministic, and frontend build passes.
- Remaining boundary: `generation_upgrade::open_and_validate_v2` is still the compatibility adapter around older generation-upgrade and secret-baseline executors. That is acceptable only because global policy now lives in probe/planner/typed recovery, and the old functions are used as step implementations rather than independent startup policy.

This means the plan is mature enough for the current product slice. It is not a heavy migration framework; it is a small route contract that future releases must reuse.

## 2026-07-31 Maturity Recheck

Verdict: the upgrade direction is mature, but only if the implementation treats the planner as the product contract and legacy functions as replaceable step executors. It should not require a new pile of startup patches for every release.

The mature rule is:

```text
future release = one registered state transition + one postcondition + one fixture test + release note
```

The immature rule, which is forbidden, is:

```text
future release = another if schema == N branch in startup
```

### Clear Upgrade Route

The supported route is deliberately narrow:

```text
schema >= 15
  -> read-only probe
  -> pure plan
  -> execute ordered schema/secret/metadata steps
  -> verify SQLite + compatibility schema + secret decryptability
  -> ready

schema < 15 or unknowable metadata
  -> typed recovery
```

For the current schema window:

```text
15 legacy secrets -> structural schema 16 -> secret baseline -> schema 17 ready
16 legacy secrets -> secret baseline -> schema 17 ready
17 encrypted secrets -> verify -> ready
```

There should be exactly one automatic path from every supported state to latest. Recovery paths are typed exits, not hidden repair branches.

### Allowed Future Change Surface

Adding a normal schema `18` should touch only:

| Area | Expected Change |
|---|---|
| SQL migration | add `0018_*.sql` |
| route registry/planner data | add `Schema 17 -> 18` step or make latest target resolve from the migration registry |
| tests | add/update `15 -> latest` and `17 -> 18 -> latest` coverage |
| docs | declare latest schema `18` and unchanged minimum baseline `15` |

Adding a secret format `2` should touch only:

| Area | Expected Change |
|---|---|
| secret migration step | add `SecretFormat 1 -> 2` |
| key preconditions | declare missing-key and wrong-key behavior |
| verification | prove all migrated secret rows decrypt |
| tests | add success, missing-key, wrong-key, interruption, and idempotency tests |
| docs | declare latest secret format `2` |

If a future version requires editing Tauri setup, frontend recovery routing policy, or unrelated migration code, that is a design smell. It must be justified before implementation.

### Current Code Audit

What is already moving in the right direction:

- `startup_probe.rs` gives a read-only startup fact source.
- `startup_upgrade_plan.rs` gives one pure planner and sets `MINIMUM_AUTOMATIC_SCHEMA_BASELINE = 15`.
- `generation_upgrade.rs` now runs structural schema work before secret baseline conversion for schema `15`.
- recovery reasons now include missing key, key mismatch, corrupted database, interrupted upgrade, schema migration failure, secret baseline failure, internal upgrade error, unsupported schema, and inconsistent metadata.
- `__secret_format_version`, `__active_key_id`, and `__last_successful_startup_version` have started separating secret/key state from table schema state.

What still risks turning into technical debt:

- `generation_upgrade::open_and_validate_v2` still owns too much execution detail. It now executes the planned steps in order, but future work should keep shrinking it toward a thin executor.
- `baseline_conversion::baseline_precondition_state` must remain a local precondition check only. It must never decide the global supported schema window.
- generation upgrade and secret baseline conversion still share `UPGRADE_JOURNAL_FILE`, but journal kind is now explicit before recovery routing.
- `StartupUpgradeError::from(String)` maps unknown strings to `internalUpgradeError`; known upgrade failures must continue to be typed before they cross module boundaries.
- release docs now declare the supported baseline, latest schema, secret format, and recovery behavior.

### Not Too Heavy Rule

Do not build a migration framework. The acceptable implementation is a small static contract:

```rust
enum UpgradeStep {
    EnsureStructuralPreBaseline,
    EnsureSecretBaseline,
    OpenRuntime,
    VerifyWritableRuntime,
    VerifySecrets,
}
```

This can later grow into data-carrying variants such as `SqlSchema { from, to }` and `SecretFormat { from, to }`, but it should remain static, typed, and testable. No dynamic workflow engine, no plugin migrations, no generic repair DSL.

### Hard Stop Checklist

A schema or secret-format release is not allowed to merge if any of these are true:

- a new version adds a top-level startup branch;
- a low-level migration function decides global support policy;
- a known failure reaches the UI through string matching;
- an existing encrypted database can create a fresh key automatically;
- runtime is registered before final verification;
- the `15 -> latest` fixture is not exercised;
- release docs do not state the minimum automatic baseline and latest target.

## Lightweight Constraint

The upgrade system must stay boring and small:

- one probe output type;
- one planner;
- one executor surface;
- one ordered route registry;
- one recovery reason enum;
- one journal mechanism for risky steps.

Adding schema `18` should normally mean adding one SQL migration, one route step, and one fixture/golden test. If it requires editing startup orchestration, that is a design smell and must be justified in the spec before implementation.

Adding secret format `2` should normally mean adding one secret-format step, key precondition tests, post-conversion verification, and a typed recovery mapping. It must not teach table migrations about secret encryption internals.

## Old-Code Cleanup Gate

The existing code is allowed to remain temporarily only as step implementation. It must not keep owning global decisions.

Completion audit:

- `generation_upgrade::open_and_validate_v2` executes planned steps in order, including future `EnsureLatestSchema`, then final runtime and secret verification.
- `baseline_conversion::baseline_precondition_state` no longer owns the global supported schema window.
- shared journal parsing returns an explicit persistence journal kind before recovery routing.
- known startup upgrade failures no longer collapse into `OpenOrMigrationFailed` on the schema15 upgrade path.
- release docs state minimum baseline, latest schema, secret format, and recovery behavior.

The old code is not fully deleted, but it has been demoted to step implementation. If a future change adds another top-level startup branch, this audit becomes invalid and must be revisited before merge.

## Background

After the encrypted secret system was merged, existing local databases can enter recovery mode during startup. The concrete case found during diagnosis is:

- The local database is at schema `15`.
- The encrypted secret baseline conversion currently treats only schema `16` or `17` as valid input.
- Startup runs the encryption baseline conversion before the ordinary SQL schema migration can advance `15 -> 16`.
- The app therefore sees a supported historical database as an invalid encryption baseline and routes into recovery.

This is not only a one-line version bug. The deeper design problem is that one `schema_version` is carrying too many meanings:

- table structure version;
- secret/encryption format version;
- whether the encrypted baseline has been applied;
- whether a device key is expected and valid.

That coupling makes startup order fragile. A future table migration, secret migration, or key-store change can accidentally break old data even when every single migration is locally correct.

## Decision

Relay Pool Desktop will treat schema `15` as the minimum supported automatic upgrade baseline.

From schema `15` onward, startup must support deterministic automatic upgrade to the current version. Databases below schema `15` are not silently repaired and are not upgraded by guessing. They enter a clear unsupported-version recovery state with an explanation and user-controlled export/backup choices.

The upgrade path must be represented as a reusable route:

```text
probe -> plan -> execute -> verify -> startup ready
```

Startup code must not grow into scattered version checks. New migrations add declarative steps to the route, while the orchestration remains stable.

## Maturity Verdict

The direction is mature if, and only if, future versions follow a fixed upgrade contract:

```text
supported baseline -> ordered registry steps -> latest verified state
```

It is not mature if every release adds another startup branch such as:

```text
if schema == x { special case }
if key missing { maybe recreate }
if conversion failed { try another path }
```

Mature products still add migrations. The difference is that migrations are data-shape changes registered in one route, not scattered rescue patches in startup. A future release may add one schema step or one secret-format step, but it must not add a new independent startup path.

## Anti Patch Rules

These rules are mandatory. They exist to prevent the upgrade system from becoming a version-by-version pile of emergency logic.

1. Startup orchestration is stable.

   New releases must not add new top-level startup branches for individual schema versions. Startup always runs:

   ```text
   probe -> plan -> execute -> verify -> ready/recovery
   ```

2. The planner owns version decisions.

   Low-level migration functions may validate their immediate preconditions, but they must not decide global routing. For example, encrypted baseline conversion can say "I require structural schema 16", but only the planner decides how schema 15 reaches that prerequisite.

3. Migrations are append-only inside the supported window.

   A normal schema change adds one ordered migration step. It does not modify old migration semantics unless fixing a proven broken unreleased migration. Released migrations are treated as immutable history.

4. Compatibility windows are explicit.

   Schema `15` is the current minimum automatic baseline. Moving that baseline requires a documented release decision, migration test evidence, and a user-facing unsupported-version recovery path.

5. Secret/key format changes are separate from table schema changes.

   A table-only migration must not need to know encryption details. A secret-format migration must declare key requirements and verification rules.

6. Every new step has a postcondition.

   A migration is not complete because its SQL ran. It is complete only when its declared verification passes.

7. Recovery reasons are typed.

   UI routing must not depend on matching arbitrary error strings. New failure classes must be added to the recovery enum and test matrix.

8. No automatic key reset for existing data.

   Key creation is a first-run operation. Existing database plus missing or wrong key is recovery, never silent repair.

9. Old-code retirement is part of the work.

   A release cannot be considered done if it adds the new route but leaves the old route making independent policy decisions for the same state.

## Upgrade Route Registry

The reusable route should be represented by a static registry owned by the data-store startup layer.

```rust
struct UpgradeRouteStep {
    id: &'static str,
    domain: UpgradeDomain,
    from: UpgradeStatePredicate,
    to: UpgradeStatePredicate,
    requires_key: KeyRequirement,
    risk: UpgradeRisk,
    verify: VerifyStep,
}

enum UpgradeDomain {
    Schema,
    SecretFormat,
    Metadata,
    Generation,
}
```

This registry is the source of truth for upgrade order. It should answer these questions without reading the UI or starting the runtime:

- What is the minimum supported automatic baseline?
- What is the latest target schema and secret format?
- Which steps are needed from a probed state?
- Which steps require an existing key?
- Which steps require backup/journal protection?
- Which verification checks must pass before startup is ready?

Future versions extend this registry. They do not extend startup composition.

## Version Terminology

The implementation must keep three version concepts distinct. Mixing them is one of the causes of the current failure.

| Version | Storage | Meaning | Who Updates It |
|---|---|---|---|
| SQL migration ledger version | `_sqlx_migrations.version` | which SQL files have run | `sqlx::Migrator` |
| Compatibility schema version | `persistence_schema_compatibility.schema_version` | which database state the app may read/write as complete | migration finalization code |
| Secret format version | app metadata or inferred legacy state | whether secret rows are legacy, encrypted baseline, or future format | secret-format migration |

For ordinary table migrations, the SQL migration ledger and compatibility schema usually advance together.

For encrypted secret baseline, they intentionally do not advance together:

```text
0017_encrypted_secret_baseline.sql
  -> adds transitional secret columns/tables
  -> does not commit compatibility schema 17

secret baseline conversion
  -> re-encrypts and validates secret rows
  -> commits compatibility schema 17
  -> commits secret_format_version 1
```

The planner must reason about all three facts. A database with SQL migration ledger `17` but compatibility schema `16` is not "latest ready"; it is a transitional state that still needs secret baseline conversion or recovery.

## Route Invariants

Every supported route must obey these invariants:

- `compatibility_schema_version < minimum_baseline` means `unsupported_version`.
- `sql_migration_version > compatibility_schema_version` is allowed only for a declared transitional step.
- Transitional steps must have a corresponding verifier and recovery route.
- `secret_format_version` may advance only after all secret rows decrypt with the expected key and final constraints pass.
- `compatibility_schema_version` may advance to latest only after both SQL shape and secret format postconditions pass.
- `Ready` requires writable compatibility, current SQL ledger, current secret format, valid key state, and successful final verification.

## Route Examples

The plan must produce simple, inspectable routes.

```text
schema 15, secret legacy:
  backup
  schema 15 -> 16
  apply 0017 transitional secret DDL
  secret format 0 -> 1
  commit compatibility schema 17
  verify schema latest
  verify secrets decrypt
  ready
```

```text
schema 16, secret legacy:
  backup
  apply 0017 transitional secret DDL
  secret format 0 -> 1
  commit compatibility schema 17
  verify schema latest
  verify secrets decrypt
  ready
```

```text
compatibility schema 17, secret format 1:
  verify schema latest
  verify secrets decrypt
  ready
```

```text
sql ledger 17, compatibility schema 16, secret legacy:
  resume or rerun secret baseline conversion
  commit compatibility schema 17 only after verification
  ready
```

```text
schema 14:
  unsupported_version
```

The important property is that there is one route from each supported state to latest. There is no independent patch path per historical version.

## Future Version Policy

For a future schema `18` release:

- add `0018_*.sql`;
- add one `Schema 17 -> 18` registry entry;
- add or update one `15 -> latest` fixture test;
- add one idempotency/current-version test;
- document latest schema `18` in release notes.

For a future secret format `2` release:

- add one `SecretFormat 1 -> 2` registry entry;
- declare whether it requires the active key;
- add missing-key and wrong-key tests;
- verify all secret rows after conversion;
- document latest secret format `2` in release notes.

For a future baseline move from schema `15` to a later schema:

- keep `15 -> latest` support until the announced support window ends;
- document the new minimum automatic baseline;
- add explicit `unsupported_version` UX for older databases;
- retain export/backup guidance for unsupported users;
- remove old route code only after tests and release notes prove the new baseline decision.

This is how the system stays light: each release adds a small registered step, while the planner/executor shape does not grow.

## Design Review

This design is acceptable only if the implementation also removes the current startup upgrade coupling. As written, the direction satisfies the reliability, maintainability, and extensibility goals, but the existing codebase will keep violating them until the orchestration is extracted from the legacy startup path.

### Reliability

Strengths:

- declares schema `15` as an explicit minimum automatic upgrade baseline;
- rejects schema `< 15` instead of guessing;
- separates missing key, key mismatch, corrupted database, interrupted upgrade, and unsupported version;
- requires read-only probing before mutation;
- requires backup and journal behavior for high-risk migrations;
- blocks normal runtime registration until verification passes.

Required tightening:

- The planner must be the only component allowed to decide migration order.
- Encryption baseline conversion must never run before structural schema prerequisites are satisfied.
- A failed upgrade must produce a typed recovery reason, not a generic startup failure.
- Existing encrypted databases must never trigger key creation.
- Startup must treat `schema_version` and `secret_format_version` as separate compatibility facts.

Reliability verdict: good target architecture, but not complete unless the legacy `open_and_validate_v2` flow is split.

### Maintainability

Strengths:

- keeps the upgrade engine intentionally small;
- defines clear components: probe, planner, executor, recovery router;
- gives future migrations a stable authoring contract;
- avoids a general workflow engine.

Required tightening:

- Current code still mixes generation migration, schema migration, secret migration, key validation, journal recovery, and runtime opening in the same path.
- Error strings are still the main boundary in several upgrade paths; the design needs typed internal errors mapped to stable recovery reasons.
- Baseline conversion currently owns schema eligibility checks that should belong to the planner.
- Future SQL migrations need one registry view that can be tested without booting the full app.

Maintainability verdict: good if implemented as a strangler refactor around the old startup code, poor if implemented as another condition inside the old functions.

### Extensibility

Strengths:

- supports future schema and secret-format changes without overloading `schema_version`;
- allows key lifecycle evolution through `key_id` and typed key errors;
- keeps unsupported historical schemas out of the hot path;
- can add future steps through a registry instead of rewriting startup.

Required tightening:

- The first version can infer `secret_format_version` from existing tables/columns, but the long-term model needs explicit metadata.
- Migration steps need machine-checkable preconditions and postconditions.
- Release notes must declare the supported baseline and target versions every time schema or secret format changes.

Extensibility verdict: enough for the next several schema/secret upgrades if the registry is kept static and typed.

## Existing Technical Debt

The existing code already contains useful reliability pieces, so the remediation should not be a rewrite. The target is to move decisions out of legacy paths and reuse the good low-level mechanisms.

Current assets to keep:

- verified SQLite backup and read-only validation;
- upgrade journals and adjacent phase transitions;
- atomic database publish helpers;
- installation lease boundary;
- device key error classification;
- schema compatibility metadata.

Current debt to retire:

| Area | Problem | Direction |
|---|---|---|
| `generation_upgrade::open_and_validate_v2` | mixes encrypted baseline conversion, runtime open, schema upgrade, health check, and secret validation | split into planned steps and keep runtime open as the final step only |
| `baseline_conversion::baseline_precondition_state` | must only validate local secret-baseline preconditions | planner accepts schema `15`, runs SQL migration first, then calls baseline conversion |
| shared `UPGRADE_JOURNAL_FILE` | generation upgrade and encrypted baseline conversion share one journal path and infer meaning from parsing | introduce typed journal kind routing before executing recovery |
| string errors | historical paths used to collapse many failures into `OpenOrMigrationFailed` | keep known startup upgrade failures mapped into stable recovery reason enum |
| schema metadata | schema version also implies encryption state | add or infer separate secret-format metadata, then persist it |
| startup composition | app setup still has too much policy in `prepare_data_store` | move policy into `StateProbe` and `UpgradePlanner`; setup only executes the returned startup outcome |

## Legacy Code Remediation Plan

The remediation should be incremental. Each step must leave the app shippable and reduce old-code surface area.

### Stage 0: Freeze Current Behavior With Tests

Add focused regression tests before moving code:

- schema `15` database currently routes through the planned upgrade path;
- schema `16` pre-baseline database converts secrets successfully;
- schema `17` encrypted database opens without rerunning conversion;
- missing key on existing database never creates a new key;
- wrong key produces key mismatch or secret validation failure recovery;
- invalid journal never starts normal runtime;
- startup failure preserves recovery UI instead of white-screening.

This stage may add fixtures or test builders, but should not change product behavior except for making the current bug reproducible.

### Stage 1: Introduce Read-Only Probe

Create a small probe module that reads facts without mutation:

```text
src-tauri/src/services/data_store/startup_probe.rs
```

Probe output should include:

- active database path;
- `schema_version`;
- latest SQL migration version known to the binary;
- inferred `secret_format_version`;
- key requirement;
- key availability category;
- journal kind and phase;
- SQLite integrity status.

Legacy functions may still execute the actual upgrade, but they must consume probe facts instead of independently rediscovering global state.

### Stage 2: Add Planner Before Executor

Create a pure planner:

```text
src-tauri/src/services/data_store/startup_upgrade_plan.rs
```

The planner converts probe facts into:

- `FreshInstall`;
- `OpenReady`;
- `Upgrade { steps }`;
- `NeedsRecovery { reason }`;
- `Conflict`.

At this stage, executor steps can call existing functions. The important change is that legacy functions stop deciding whether they are allowed to run.

### Stage 3: Split `open_and_validate_v2`

Replace the current monolithic flow with explicit executor steps:

```text
ensure_structural_schema
ensure_secret_baseline
open_runtime
verify_runtime_health
validate_secrets
```

The order must be:

```text
probe schema/key
ordinary SQL migration if schema is 15 or 16
encrypted secret baseline conversion if secret format is legacy
open runtime
validate writable health
validate secrets
```

After this stage, schema `15` can no longer be rejected by encrypted baseline conversion before the SQL migration runs.

### Stage 4: Type Recovery Errors

Introduce internal error types:

```rust
enum StartupUpgradeError {
    UnsupportedVersion,
    MissingKey,
    KeyMismatch,
    CorruptedDatabase,
    InterruptedUpgrade,
    SchemaMigrationFailed,
    SecretBaselineFailed,
    Internal,
}
```

Map them once into `RecoveryReason`. Do not let low-level string messages decide UI behavior.

### Stage 5: Persist Secret Format Metadata

Add explicit metadata after the route is stable:

```text
secret_format_version = 1
active_key_id = ...
last_successful_startup_version = ...
```

Until this migration lands, `secret_format_version` may be inferred from existing encrypted columns and baseline constraints. After it lands, inference becomes a backward-compatibility path only.

### Stage 6: Retire Old Decision Branches

Once probe, planner, executor, and recovery routing own startup policy:

- keep generation-1 import code only as a step implementation;
- keep baseline conversion only as a secret-format step implementation;
- remove duplicated schema-state checks from baseline conversion;
- keep generic `OpenOrMigrationFailed` out of known startup upgrade failures;
- make startup setup register either `Ready` or typed `Recovery`, never a half-ready runtime.

## Acceptance Gates

The design is implemented only when these gates pass:

- `15 -> latest` upgrade is covered by an automated Rust test.
- Baseline conversion is unreachable for schema `15` until SQL migration has advanced the database.
- Existing database plus missing key has zero key-create calls.
- All known upgrade failures map to typed recovery reasons.
- Normal app runtime is registered only after final verification.
- The old monolithic startup path is either deleted or reduced to a compatibility adapter with no policy decisions.
- Release docs state the minimum automatic baseline and latest target schema.

## Non Goals

- Do not support arbitrary ancient databases forever.
- Do not auto-generate a new encryption key for an existing database.
- Do not merge rows across multiple user databases during recovery.
- Do not treat recovery mode as a magic repair button.
- Do not make every future change add a bespoke startup patch.
- Do not block ordinary table-only migrations on secret/key migration internals.

## Version Model

The app should separate these concepts:

| Field | Meaning | Example |
|---|---|---|
| `schema_version` | SQLite table/data shape | `15`, `16`, `17`, future values |
| `secret_format_version` | How secrets are stored/encrypted | `0` for legacy/plain or absent baseline, `1` for encrypted baseline |
| `key_id` | Which local device key protects current secrets | UUID or stable local id |
| `upgrade_phase` | Whether a high-risk upgrade is in progress | `none`, `planning`, `running`, `verifying`, `failed` |
| `last_successful_startup_version` | Last app version that fully opened this database | SemVer string |

`schema_version` remains the table migration counter. It must not be used as the only signal for encryption state.

For the current recovery issue, schema `15` is a valid starting point. The correct route is:

```text
schema 15
  -> run ordinary schema migrations to the pre-encryption structural baseline
  -> run encrypted secret baseline conversion when the key state is valid
  -> mark secret_format_version as encrypted
  -> verify
  -> ready
```

## Supported Upgrade Window

Mature projects usually define a support window instead of promising infinite direct upgrades. For this project:

- Minimum automatic baseline: schema `15`.
- Current target: latest schema in the migration registry.
- Unsupported: schema `< 15`, missing metadata that cannot be safely interpreted, or a database that fails SQLite integrity checks.
- Future rule: when the minimum baseline moves, it must be an explicit release decision documented in release notes and migration docs.

This keeps the system small. We do not need to keep every historical branch alive forever, but we do need one reliable path from the declared baseline.

## Upgrade Engine

The upgrade engine should be thin and boring.

### StateProbe

Read-only inspection. It must not create files, mutate schema, create keys, or run migrations.

It gathers:

- database exists and is readable;
- SQLite header and `PRAGMA quick_check`;
- current `schema_version`;
- whether app metadata tables exist;
- current `secret_format_version`, if available;
- whether encrypted secret rows exist;
- whether a key is required;
- whether the expected key exists and can be opened;
- whether an upgrade journal is present.

### UpgradePlanner

Pure planning from probed facts. Same input must produce the same plan.

Example plans:

```text
FreshInstall:
  create database
  initialize latest schema
  create first device key
  mark ready

UpgradeFrom15:
  backup
  schema 15 -> 16
  schema 16 -> 17 structural steps
  secret format 0 -> 1
  verify

Ready:
  verify current schema and secret format
  start app

NeedsRecovery:
  route to explicit recovery reason
```

The planner owns ordering. Individual migrations should not decide global startup policy.

### UpgradeExecutor

Runs the planned steps with journal support for risky operations.

Required behavior:

- take a verified backup before destructive or irreversible changes;
- run steps in a transaction when SQLite allows it;
- write an upgrade journal before high-risk phases;
- resume or fail clearly after interruption;
- never continue with a new key if an existing encrypted database expects a different key;
- verify final schema and secret state before exposing normal app runtime.

### RecoveryRouter

Maps failures to user-understandable categories:

| Reason | Meaning | Behavior |
|---|---|---|
| `unsupported_version` | database is older than schema `15` or metadata is unknowable | block auto-upgrade, allow backup/export guidance |
| `missing_key` | existing encrypted database cannot find its expected key | do not create a new key, ask user to restore key/device data |
| `key_mismatch` | a key exists but cannot decrypt expected data | block startup, show key mismatch recovery |
| `corrupted_db` | SQLite cannot pass integrity/read checks | block startup, offer backup/location actions |
| `interrupted_upgrade` | journal shows incomplete upgrade | resume when safe, otherwise show explicit failed step |
| `internal_upgrade_error` | migration code failed unexpectedly | block runtime and preserve diagnostics |

Recovery mode should explain what is wrong. It should not pretend every problem can be repaired automatically.

## Key Lifecycle Rules

The key rule is simple:

> New key generation is allowed only for a proven fresh install.

For existing databases:

- missing key means recovery, not key creation;
- key mismatch means recovery, not reset;
- old plaintext/legacy secrets may be converted only after the planner proves the database is in a supported pre-encryption state;
- the migration writes `key_id` and `secret_format_version` only after encryption verification succeeds.

This prevents the most dangerous failure mode: opening an old database with a newly generated key and making all previous encrypted data unrecoverable.

## Journal

The journal can be very small. It exists only for risky, multi-step upgrades.

Suggested table:

```sql
CREATE TABLE IF NOT EXISTS app_upgrade_journal (
  upgrade_id TEXT PRIMARY KEY,
  target_schema_version INTEGER NOT NULL,
  target_secret_format_version INTEGER NOT NULL,
  current_step TEXT NOT NULL,
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_error_code TEXT
);
```

Rules:

- no secret values in the journal;
- no API keys, cookies, tokens, or decrypted payloads;
- journal status must be checked before normal startup;
- completed journals can be compacted or retained as non-sensitive audit data.

## Migration Authoring Contract

Each future migration should declare:

| Field | Purpose |
|---|---|
| `id` | stable migration id |
| `from` | required source version/state |
| `to` | target version/state |
| `kind` | `schema`, `secret_format`, `metadata`, or `repair` |
| `requires_key` | whether a valid existing key is needed |
| `transactional` | whether it can run in a single SQLite transaction |
| `verify` | postcondition check |
| `reversible` | whether rollback is possible or backup is mandatory |

The implementation does not need a heavy framework. A Rust enum plus a static registry is enough:

```rust
enum UpgradeStep {
    SqlSchema { from: u32, to: u32, migration_id: &'static str },
    SecretBaseline { from_format: u32, to_format: u32 },
    Verify { check_id: &'static str },
}
```

The important part is that steps are planned centrally and verified consistently.

## Baseline 15 Route

The schema `15` route is the first supported compatibility contract:

```text
Input:
  schema_version >= 15

Route:
  1. Probe database and key state read-only.
  2. If schema < 15, stop with unsupported_version.
  3. If schema is 15 or 16, run ordinary schema migrations in order.
  4. After structural schema reaches the encryption baseline requirement, run secret baseline conversion.
  5. Write secret_format_version/key metadata only after conversion verifies.
  6. Run final integrity and app metadata checks.
  7. Start normal runtime only after all checks pass.
```

This route fixes the current class of bug because encryption conversion no longer rejects schema `15` before schema migration has a chance to run.

## How To Keep It Lightweight

Keep the design small by enforcing boundaries:

- one probe function;
- one planner;
- one executor;
- one recovery reason enum;
- one migration registry;
- one tiny journal table for high-risk steps.

Avoid building:

- a visual migration editor;
- a generic workflow engine;
- row-level data merge;
- cloud backup;
- dynamic plugin migrations;
- permanent support for every historical schema.

The system should feel like a strict startup checklist, not a platform.

## Required Tests

Minimum regression tests:

- schema `15 -> latest` succeeds;
- schema `16 -> latest` succeeds;
- schema `17/latest` opens without rerunning secret baseline;
- schema `< 15` returns `unsupported_version`;
- existing encrypted database with missing key returns `missing_key`;
- existing encrypted database with wrong key returns `key_mismatch`;
- interrupted secret baseline either resumes safely or returns `interrupted_upgrade`;
- final runtime is not registered when upgrade fails.

For the current bug, the most important test is:

```text
Given a schema 15 database with legacy secret state
When startup runs
Then ordinary schema migrations run before encrypted secret baseline conversion
And the app reaches ready state with latest schema and encrypted secrets
```

## Implementation Plan

1. Add read-only startup probing for schema, secret format, key requirement, and journal state.
2. Introduce a small `UpgradePlan` type and central planner.
3. Change startup so schema `15` is accepted as a supported baseline.
4. Ensure ordinary SQL migrations run before encrypted secret baseline conversion when starting from schema `15`.
5. Add explicit recovery reasons for unsupported version, missing key, key mismatch, corruption, and interrupted upgrade.
6. Add focused Rust tests for `15 -> latest`, key mismatch, and interrupted upgrade.
7. Only after this route is stable, consider adding persistent `secret_format_version` metadata if it is not already present.

## Release Rule

A release that changes schema, secret format, or key lifecycle must document:

- new latest schema version;
- minimum automatic upgrade baseline;
- whether `secret_format_version` changed;
- whether existing keys are reused or migrated;
- recovery behavior for unsupported or failed upgrades;
- tests proving the declared baseline upgrades successfully.

This makes the upgrade route clear and reusable without turning every release into a pile of emergency patches.
