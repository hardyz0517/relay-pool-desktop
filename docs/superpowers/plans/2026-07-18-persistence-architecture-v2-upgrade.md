# Persistence Architecture V2 Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 21,000-line `AppDatabase` persistence god object with a bounded SQLx SQLite modular monolith that has explicit use-case transactions, deterministic generation upgrade, cross-process ownership, binary/schema compatibility, and executable architecture boundaries.

**Architecture:** Build the V2 kernel offline, migrate one vertical business slice at a time behind narrow application services and consumer-owned ports, import generation 1 into a separately validated generation-2 database, then perform one production cutover and delete every legacy persistence path. `InstallationLease`, `SchemaCompatibility`, `RecoveryPlanner`, and the database unique constraints are single-purpose authorities; there is no generic repository, actor framework, workflow DSL, dual write, or permanent facade.

**Tech Stack:** Rust 1.89+ (current workspace toolchain verified as 1.95), Tauri 2, Tokio 1, SQLx 0.8.6 with SQLite/macros/migrate/offline metadata, SQLite WAL, `thiserror` 2, `semver` 1, `zeroize` 1, serde, CodeGraph/tree-sitter structural checks, Node contract tests, Cargo tests, React/TypeScript/Vite release verification.

**Approved design:** `docs/superpowers/specs/2026-07-18-persistence-architecture-v2-upgrade-design.md`

---

## Execution Rules

- Before Task 0, invoke `superpowers:using-git-worktrees`. Create a `codex/persistence-v2-upgrade` worktree only after the current Proxy/Stations changes have a durable commit or an explicitly approved patch; never discard or silently absorb the dirty hunks currently present in `src-tauri/src/services/proxy/**`, `src/features/stations/StationsPage.tsx`, capability defaults, or generated output.
- At the start and end of every task, record `git status --short`, `git log -5 --oneline`, the main-checkout HEAD, and the worktree HEAD. Merge committed mainline drift before the task; do not import uncommitted mainline state by assumption.
- Use RED-GREEN TDD. Observe each named test fail for the expected missing behavior before implementation, then run the focused test, the affected module suite, and the task gate.
- Never use `git add .`, `git add -A`, or `git commit -a`. Stage only paths listed by the task. If a listed path contains unrelated hunks, use `git add -p` and verify `git diff --cached --name-only` plus `git diff --cached`.
- Do not expose SQLx types outside `persistence`, pass `AppServices` below the composition root, add a generic `Repository<T>`, or put business methods on `WriteSession`, `InstallationLease`, `RecoveryPlanner`, or the dispatcher.
- No production dual write. Differential reads may run only against cloned fixture databases; V1 and V2 writes always run in separate fixture copies.
- Tasks 5-11 build V2 services, command adapters, and consumer-port adapters offline. They compile under integration/test composition but are not registered by Tauri; the existing V1 adapter remains the sole production path. Task 14 performs the only production composition swap, and no intermediate branch build is published.
- A task may add a temporary adapter only when this plan names its owner and deletion task. Every temporary adapter is deleted in Task 16.
- Push is not part of this plan. Each task creates a local, reviewable commit after its staged snapshot passes.

**Checklist integrity note (2026-07-21):** Tasks 0-13 retain their original execution checkboxes. They are not retroactively marked complete without task-scoped RED/GREEN and commit evidence. Current implementation state is recorded only in the dated status notes for Tasks 14-17 and in the qualification audit; an unchecked historical step is not, by itself, evidence that its production behavior is absent.

## Target File Map

| Path | Final responsibility |
|---|---|
| `src-tauri/src/application/app_services.rs` | Composition-only container of narrow service handles; no business methods |
| `src-tauri/src/application/error.rs` | Typed application failures and boundary-safe classifications |
| `src-tauri/src/application/clock.rs` | Production UTC clock and deterministic test clock |
| `src-tauri/src/application/ids.rs` | Durable ID generation and deterministic test generator |
| `src-tauri/src/application/*.rs` | One consistency/use-case area per service |
| `src-tauri/src/application/queries/*.rs` | Named, bounded workflow read models |
| `src-tauri/src/persistence/runtime.rs` | SQLx pool lifecycle and readiness only |
| `src-tauri/src/persistence/schema_compatibility.rs` | The only binary/schema read-write compatibility decision |
| `src-tauri/src/persistence/write_coordinator.rs` | Fair single-writer admission and metrics |
| `src-tauri/src/persistence/write_session.rs` | Opaque non-clone SQLx transaction owner |
| `src-tauri/src/persistence/read_session.rs` | Short-lived consistent read snapshot |
| `src-tauri/src/persistence/stores/*.rs` | SQL, private row types, explicit mappings for one consistency area |
| `src-tauri/src/persistence/legacy_import/profiles/*.rs` | Released generation-1 schema profiles only |
| `src-tauri/src/persistence/upgrade_journal.rs` | Durable six-phase journal serialization |
| `src-tauri/src/persistence/upgrade_recovery_plan.rs` | Pure observation-to-plan function |
| `src-tauri/src/persistence/upgrade_recovery_executor.rs` | Execute one plan with precondition revalidation |
| `src-tauri/src/services/data_store/installation_lease.rs` | OS file-handle lock only |
| `src-tauri/src/commands/*.rs` | Thin Tauri ACL/DTO/error adapters |
| `src-tauri/tests/persistence_*.rs` | File-backed integration, migration, recovery, differential, and architecture tests |
| `docs/superpowers/audits/persistence-v2-*.json` | Versioned schema/field/boundary/released-profile manifests |

## Shared Type Ledger

| Type | Introduced | Final owner |
|---|---:|---|
| `ApplicationError`, `Clock`, `IdGenerator` | 1 | `application/{error,clock,ids}.rs` |
| `InstallationLease`, `LeaseError` | 2 | `services/data_store/installation_lease.rs` |
| `RuntimeState`, `RuntimeLifecycle` | 2 | `persistence/runtime.rs` |
| `PersistenceError`, `PersistenceRuntime`, `PersistenceHandle` | 3 | `persistence/{error,runtime}.rs` |
| `SchemaCompatibility`, `BinaryCompatibility`, `OpenMode` | 3 | `persistence/schema_compatibility.rs` |
| `WriteCoordinator`, `WriteSession`, `ReadSession` | 4 | matching `persistence/*.rs` files |
| `StationService`, `SettingsService` | 5 | `application/{stations,settings}.rs` |
| `SecretBytes`, `CredentialVault`, `CredentialService` | 6 | `application/credentials.rs` and existing secret boundary |
| `RoutingService`, `RoutingQuery` | 7 | `application/routing.rs` |
| `RequestFinalizationService` | 8 | `application/request_finalization.rs` |
| `CollectorService` | 9 | `application/collectors.rs` |
| `PricingService`, `MonitoringService`, `PageLimit` | 10 | matching application/query modules |
| `AppServices` | 11 | `application/app_services.rs` |
| `LegacySchemaProfile`, `LegacyReadSession` | 12 | `persistence/legacy_import/**` |
| `UpgradePhase`, `ObservedUpgradeState`, `RecoveryPlan` | 13 | upgrade journal/planner modules |
| `DatabaseGeneration`, `DataDirConfigV3` | 14 | `services/data_store/config.rs` |

## Program Exit Gates

| Contract | Primary tasks | Required evidence |
|---|---:|---|
| Cross-process single mutable owner | 1, 14, 15 | Two-process lock tests, crash release, signed-package launch-order matrix |
| Binary/schema compatibility before writes | 2, 14, 15 | Readable/writable/incompatible matrix and old-binary hard failure |
| Bounded SQLx runtime and transaction ownership | 2, 3 | Pool/PRAGMA, fairness, cancellation, rollback, snapshot tests |
| Vertical application/store boundaries | 5-11 | Real transaction integration tests and AST dependency gate |
| Proxy finalization exactly once | 8 | EOF/error/drop/shutdown/failure tests and database uniqueness |
| Deterministic generation upgrade | 12, 13 | Released-schema fixtures, six-phase crash matrix, tombstone tests |
| DataDirConfig V2-to-V3 and relocation safety | 13, 14 | Atomic config tests, active/pending/source preservation, mutual exclusion |
| Secret/AAD lifecycle and plaintext removal | 6, 12, 15 | Vault tests, canary scans, no plaintext V2 columns |
| Bounded named read models | 7, 10 | Pagination, deterministic order, snapshot, EXPLAIN QUERY PLAN evidence |
| Typed errors and operational diagnostics | 1-4, 8, 13, 15 | Stable classification, redaction, retry budget, health events |
| Future V2 migration discipline | 3, 12, 15 | Offline metadata, checksum, pre-migration backup, compatibility matrix |
| Executable architecture fitness | 0, 1, 11, 16 | Visibility, boundary manifest, AST graph, fan-in/fan-out baseline |
| One production persistence architecture | 14, 16 | No selector, dual write, `AppDatabase`, `database.rs`, or `rusqlite` |
| Performance/security/release quality | 15, 17 | p95 gates, canary scans, signed Windows matrix, final staged snapshot |

## Workstream A: Baseline And Kernel

### Task 0: Freeze the migration ledger and dirty baseline

**Files:**
- Create: `docs/superpowers/audits/persistence-v2-boundary-manifest.json`
- Create: `docs/superpowers/audits/persistence-v2-field-ledger.json`
- Create: `docs/superpowers/audits/persistence-v2-released-schema-manifest.json`
- Create: `docs/superpowers/audits/2026-07-18-persistence-v2-baseline.md`
- Read only: `src-tauri/src/services/database.rs`
- Read only: `src-tauri/src/commands/mod.rs`
- Read only: `src-tauri/src/services/{proxy,collectors,channel_monitors,data_store}/**/*.rs`

- [ ] **Step 1: Record repository and drift state**

Run:

```powershell
git status --short
git log -5 --oneline
git -C D:\Dev\Projects\relay-pool-desktop status --short
git -C D:\Dev\Projects\relay-pool-desktop log -5 --oneline
codegraph status .
```

Expected: the worktree has no task-unrelated modifications; the main checkout's current dirty paths are recorded in the baseline, not copied or reverted.

- [ ] **Step 2: Capture the structural blast radius**

Run CodeGraph queries and store the machine-readable symbol/edge lists in the three JSON manifests:

```powershell
codegraph query AppDatabase --json > $env:TEMP\persistence-v2-appdatabase.json
codegraph query DataDirConfigV2 --json > $env:TEMP\persistence-v2-config.json
codegraph query FinalizationDispatcher --json > $env:TEMP\persistence-v2-finalization.json
```

The boundary manifest must list every current `AppDatabase` consumer grouped into `command`, `proxy`, `collector`, `monitor`, `query`, `settings`, and `test`; the field ledger must list every protected table/column with owner, writer, reader, sensitivity, retention, and V2 disposition; the released-schema manifest must map the supported `v0.3.1` schema to a profile and fixture hash and record earlier tags as explicitly unsupported. No entry may use `unknown owner`; unsupported fields use disposition `drop-with-evidence` and name the proof query. Existing user-visible history defaults to `retain-until-explicit-user-cleanup`; this upgrade adds no automatic deletion policy merely to reduce fixture size.

Use this stable top-level boundary-manifest shape so later tests and staging commands consume the same names:

```json
{
  "version": 1,
  "allowed_exports": {},
  "allowed_edges": [],
  "temporary_legacy_consumers": [],
  "temporary_legacy_paths": [],
  "allowed_consumers": {
    "persistence::legacy_import": ["lib"]
  },
  "fan_in_baseline": {},
  "fan_out_baseline": {}
}
```

Populate every array/map from CodeGraph evidence in this task; empty arrays above describe the schema, not the final manifest contents.

- [ ] **Step 3: Run and record the baseline gates**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --nocapture
pnpm.cmd test:contracts
pnpm.cmd build
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: record exact pass/fail counts and durations. Existing failures block implementation until classified as mainline drift or fixed outside this migration; do not bless a red baseline.

- [ ] **Step 4: Commit the evidence-only baseline**

```powershell
git add -- docs/superpowers/audits/persistence-v2-boundary-manifest.json docs/superpowers/audits/persistence-v2-field-ledger.json docs/superpowers/audits/persistence-v2-released-schema-manifest.json docs/superpowers/audits/2026-07-18-persistence-v2-baseline.md
git diff --cached --check
git commit -m "docs: freeze persistence v2 migration baseline"
```

### Task 1: Add architecture fitness gates and module skeletons

**Files:**
- Create: `src-tauri/src/application/mod.rs`
- Create: `src-tauri/src/application/error.rs`
- Create: `src-tauri/src/application/clock.rs`
- Create: `src-tauri/src/application/ids.rs`
- Create: `src-tauri/src/persistence/mod.rs`
- Create: `src-tauri/tests/persistence_architecture.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

- [ ] **Step 1: Write the failing AST boundary test**

Add `syn = { version = "2", features = ["full", "visit"] }` as a dev dependency and parse Rust modules structurally. The test must load `persistence-v2-boundary-manifest.json` and fail on forbidden imports or unregistered public exports:

```rust
#[test]
fn persistence_v2_dependency_edges_match_the_boundary_manifest() {
    let graph = ParsedModuleGraph::load("src").expect("parse Rust module graph");
    let manifest = BoundaryManifest::load(
        "../docs/superpowers/audits/persistence-v2-boundary-manifest.json",
    ).expect("load boundary manifest");

    graph.assert_no_dependency_cycles();
    graph.assert_forbidden_prefix_absent("application", "tauri");
    graph.assert_forbidden_prefix_absent("application", "sqlx");
    graph.assert_forbidden_prefix_absent("commands", "sqlx");
    graph.assert_forbidden_prefix_absent("services::proxy", "persistence::stores");
    graph.assert_public_exports_equal(&manifest.allowed_exports);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture
```

Expected: FAIL because `ParsedModuleGraph`, the new modules, and their manifest exports do not exist.

- [ ] **Step 3: Implement only the closed shared types**

Use these exact error and dependency contracts; do not add a service base class:

```rust
#[derive(Debug, thiserror::Error)]
pub(crate) enum ApplicationError {
    #[error("persistence unavailable")]
    Unavailable,
    #[error("installation already running")]
    InstallationAlreadyRunning,
    #[error("resource busy")]
    Busy,
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("stale revision")]
    StaleRevision,
    #[error("constraint violation")]
    ConstraintViolation,
    #[error("migration failed")]
    MigrationFailed,
    #[error("unsupported legacy schema")]
    UnsupportedLegacySchema,
    #[error("integrity failed")]
    IntegrityFailed,
    #[error("secret validation failed")]
    SecretValidationFailed,
    #[error("I/O failed")]
    IoFailed,
    #[error("cancelled")]
    Cancelled,
    #[error("schema incompatible")]
    IncompatibleSchema,
    #[error("commit outcome unknown")]
    CommitOutcomeUnknown,
    #[error("recovery precondition changed")]
    RecoveryPreconditionChanged,
    #[error("internal failure")]
    Internal,
}

pub(crate) trait Clock: Send + Sync {
    fn now_utc(&self) -> chrono::DateTime<chrono::Utc>;
}

pub(crate) trait IdGenerator: Send + Sync {
    fn next_id(&self) -> String;
}

pub(crate) struct SystemClock;
impl Clock for SystemClock {
    fn now_utc(&self) -> chrono::DateTime<chrono::Utc> { chrono::Utc::now() }
}

pub(crate) struct UuidV7Generator;
impl IdGenerator for UuidV7Generator {
    fn next_id(&self) -> String { uuid::Uuid::now_v7().to_string() }
}
```

Add `uuid = { version = "1", features = ["v7", "rng"] }`. Test code uses a fixed clock and sequence ID generator declared under `#[cfg(test)]`; Store code never calls system time or random APIs. Keep modules private by default. `application/mod.rs` and `persistence/mod.rs` may re-export only symbols already entered in the boundary manifest.

- [ ] **Step 4: Implement the AST graph assertions and run GREEN**

Implement `ParsedModuleGraph` with `syn::visit::Visit`; collect `use` paths, module visibility, and `pub use` exports. Do not inspect raw source strings. Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: both commands exit 0; the test reports no cycles or forbidden edges.

- [ ] **Step 5: Commit the architecture harness**

```powershell
git add -- src-tauri/src/application/mod.rs src-tauri/src/application/error.rs src-tauri/src/application/clock.rs src-tauri/src/application/ids.rs src-tauri/src/persistence/mod.rs src-tauri/tests/persistence_architecture.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock docs/superpowers/audits/persistence-v2-boundary-manifest.json
git diff --cached --check
git commit -m "test: enforce persistence v2 architecture boundaries"
```

### Task 2: Implement InstallationLease and runtime lifecycle

**Files:**
- Create: `src-tauri/src/services/data_store/installation_lease.rs`
- Create: `src-tauri/src/persistence/runtime.rs`
- Modify: `src-tauri/src/services/data_store/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Test: inline tests plus `src-tauri/tests/persistence_installation_lease.rs`

- [ ] **Step 1: Write failing lease and lifecycle tests**

```rust
#[test]
fn second_installation_lease_fails_without_mutating_data_store() {
    let root = temp_installation();
    let first = InstallationLease::try_acquire(root.config_dir()).expect("first lease");
    let before = snapshot_tree(root.path());

    let error = InstallationLease::try_acquire(root.config_dir()).unwrap_err();

    assert!(matches!(error, LeaseError::AlreadyRunning));
    assert_eq!(snapshot_tree(root.path()), before);
    drop(first);
    InstallationLease::try_acquire(root.config_dir()).expect("released by file handle");
}

#[test]
fn runtime_lifecycle_is_monotonic() {
    let state = RuntimeLifecycle::new();
    assert_eq!(state.transition(RuntimeState::Ready), Ok(()));
    assert_eq!(state.transition(RuntimeState::Starting), Err(RuntimeTransitionError::Reverse));
    assert_eq!(state.transition(RuntimeState::Draining), Ok(()));
    assert!(!state.accepts_new_work());
    assert_eq!(state.transition(RuntimeState::Closed), Ok(()));
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml installation_lease -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml runtime_lifecycle -- --nocapture
```

Expected: compile failure for missing lease and lifecycle types.

- [ ] **Step 3: Implement the narrow authorities**

Set `rust-version = "1.89"` in `[package]` and implement:

```rust
pub(crate) struct InstallationLease {
    file: std::fs::File,
}

impl InstallationLease {
    pub(crate) fn try_acquire(config_dir: &Path) -> Result<Self, LeaseError> {
        std::fs::create_dir_all(config_dir).map_err(LeaseError::Io)?;
        let path = config_dir.join("relay-pool-installation.lock");
        let file = OpenOptions::new().read(true).write(true).create(true).open(path)
            .map_err(LeaseError::Io)?;
        file.try_lock().map_err(|error| match error.kind() {
            std::io::ErrorKind::WouldBlock => LeaseError::AlreadyRunning,
            _ => LeaseError::Io(error),
        })?;
        Ok(Self { file })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeState { Starting, Ready, Draining, Closed, Unavailable }
```

Do not implement PID cleanup. Keep `tauri-plugin-single-instance` registered before `setup`; acquire the lease before `prepare_data_store` or any config/database mutation and move it into one top-level owner.

Emit only fixed events `installation_lease_acquired`, `installation_lease_contended`, and `installation_lease_released`; include outcome and elapsed milliseconds, never lock path or arbitrary OS error text.

- [ ] **Step 4: Add a real child-process lock test**

The integration test launches the current test binary in helper mode, holds the lock, asserts a second helper exits with code 23 before creating config/SQLite files, terminates the first helper, then asserts the third helper acquires the lease. Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_installation_lease -- --nocapture
node scripts/native-shell-single-instance-tray.test.mjs
```

Expected: both exit 0; no stale-lock removal occurs.

- [ ] **Step 5: Commit the lease slice**

```powershell
git add -- src-tauri/src/services/data_store/installation_lease.rs src-tauri/src/services/data_store/mod.rs src-tauri/src/persistence/runtime.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tests/persistence_installation_lease.rs
git commit -m "feat: add installation persistence lease"
```

### Task 3: Build SQLx runtime and schema compatibility gate

**Files:**
- Create: `src-tauri/src/persistence/error.rs`
- Create: `src-tauri/src/persistence/schema_compatibility.rs`
- Create: `src-tauri/src/persistence/health_check.rs`
- Create: `src-tauri/src/persistence/migrations.rs`
- Create: `src-tauri/src/persistence/migrations/0001_v2_initial.sql`
- Create: `src-tauri/tests/persistence_runtime.rs`
- Create: `scripts/prepare-sqlx.ps1`
- Modify: `src-tauri/src/persistence/runtime.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

- [ ] **Step 1: Write failing file-backed runtime tests**

```rust
#[tokio::test]
async fn writable_open_requires_compatible_schema_metadata() {
    let db = V2Fixture::create().await;
    db.set_compatibility(SchemaCompatibility {
        database_generation: 2,
        schema_version: 1,
        min_reader_app_version: Version::new(0, 4, 0),
        min_writer_app_version: Version::new(0, 4, 0),
        updated_by_migration: 1,
    }).await;

    let binary = BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=1,
        writable_schema: BTreeSet::from([1]),
    };
    let error = PersistenceRuntime::open(db.path(), binary).await.unwrap_err();
    assert!(matches!(error, PersistenceError::IncompatibleSchema { writable: false, .. }));
    assert_eq!(db.write_probe_count().await, 0);
}
```

Also cover missing database with `create_if_missing=false`, generation mismatch, readable-but-not-writable inspection, unknown future schema, metadata/SQLx mismatch, and valid writable open.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_runtime -- --nocapture
```

Expected: compile failure because SQLx runtime and compatibility types do not exist.

- [ ] **Step 3: Add pinned dependencies and the metadata table**

Use exact dependency versions in `Cargo.toml`:

```toml
sqlx = { version = "=0.8.6", default-features = false, features = ["runtime-tokio", "sqlite", "migrate", "macros", "chrono", "json"] }
thiserror = "2"
```

`semver = "1"` already exists. Migration `0001_v2_initial.sql` must create the SQLx-managed V2 schema foundation and exactly one compatibility row guarded by `CHECK (database_generation = 2)` and a unique singleton key.

- [ ] **Step 4: Implement compatibility before writable pool registration**

```rust
pub(crate) struct BinaryCompatibility {
    pub app_version: semver::Version,
    pub database_generation: i64,
    pub readable_schema: RangeInclusive<i64>,
    pub writable_schema: BTreeSet<i64>,
}

pub(crate) enum OpenMode { Writable, InspectionOnly }

#[derive(Clone)]
pub(crate) struct PersistenceHandle {
    pool: sqlx::SqlitePool,
    lifecycle: Arc<RuntimeLifecycle>,
}

pub(crate) fn decide_open_mode(
    binary: &BinaryCompatibility,
    database: &SchemaCompatibility,
    sqlx_version: i64,
) -> Result<OpenMode, PersistenceError> {
    if database.database_generation != binary.database_generation
        || database.schema_version != sqlx_version
        || !binary.readable_schema.contains(&database.schema_version)
        || binary.app_version < database.min_reader_app_version
    {
        return Err(PersistenceError::IncompatibleSchema { writable: false });
    }
    if !binary.writable_schema.contains(&database.schema_version)
        || binary.app_version < database.min_writer_app_version
    {
        return Ok(OpenMode::InspectionOnly);
    }
    Ok(OpenMode::Writable)
}
```

At this task `PersistenceHandle` exposes compatibility and health only; Task 4 adds typed begin-read/begin-write methods together with the coordinator. Its pool fields remain private and it is never handed to Proxy, Collector adapters, Command, or UI code. Open runtime pools with max 4, acquire timeout 5 seconds, idle timeout 5 minutes, WAL initialized once, and every connection configured with foreign keys, FULL synchronous, and 5-second busy timeout. Normal open must never create a missing file.

Compatibility inspection emits a fixed decision code (`writable`, `inspection_only`, `generation_mismatch`, `reader_too_old`, `writer_too_old`, `metadata_mismatch`) plus schema/app versions; it never emits database path, SQL, or row values.

- [ ] **Step 5: Generate offline metadata and run GREEN**

`scripts/prepare-sqlx.ps1` creates `output/sqlx-prepare.sqlite3`, sets a normalized SQLite `DATABASE_URL`, applies `src-tauri/src/persistence/migrations`, runs `cargo sqlx prepare` from `src-tauri`, then removes the temporary main/WAL/SHM files in `finally` and restores the prior environment variable. `-Check` runs `cargo sqlx prepare --check` instead of updating `.sqlx`.

```powershell
param([switch]$Check)
$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$output = Join-Path $repo "output"
$db = Join-Path $output "sqlx-prepare.sqlite3"
New-Item -ItemType Directory -Force -Path $output | Out-Null
foreach ($candidate in @($db, "$db-wal", "$db-shm")) {
    if (([IO.Path]::GetFullPath($candidate)).StartsWith(([IO.Path]::GetFullPath($output)))) {
        Remove-Item -LiteralPath $candidate -Force -ErrorAction SilentlyContinue
    }
}
$old = $env:DATABASE_URL
try {
    $env:DATABASE_URL = "sqlite://$($db.Replace('\', '/'))"
    Push-Location (Join-Path $repo "src-tauri")
    cargo sqlx database create
    if ($LASTEXITCODE) { throw "sqlx database create failed" }
    cargo sqlx migrate run --source src/persistence/migrations
    if ($LASTEXITCODE) { throw "sqlx migrate failed" }
    $args = @("sqlx", "prepare")
    if ($Check) { $args += "--check" }
    $args += @("--", "--all-targets")
    & cargo @args
    if ($LASTEXITCODE) { throw "sqlx prepare failed" }
} finally {
    Pop-Location -ErrorAction SilentlyContinue
    $env:DATABASE_URL = $old
    foreach ($candidate in @($db, "$db-wal", "$db-shm")) {
        Remove-Item -LiteralPath $candidate -Force -ErrorAction SilentlyContinue
    }
}
```

```powershell
cargo install sqlx-cli --version 0.8.6 --no-default-features --features sqlite,rustls
powershell -ExecutionPolicy Bypass -File scripts/prepare-sqlx.ps1
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_runtime -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: `.sqlx/` metadata is generated; all runtime matrix cases pass.

- [ ] **Step 6: Commit the runtime foundation**

```powershell
git add -- src-tauri/src/persistence/error.rs src-tauri/src/persistence/runtime.rs src-tauri/src/persistence/schema_compatibility.rs src-tauri/src/persistence/health_check.rs src-tauri/src/persistence/migrations.rs src-tauri/src/persistence/migrations/0001_v2_initial.sql src-tauri/src/persistence/mod.rs src-tauri/tests/persistence_runtime.rs scripts/prepare-sqlx.ps1 src-tauri/Cargo.toml src-tauri/Cargo.lock .sqlx
git commit -m "feat: add compatible sqlx persistence runtime"
```

### Task 4: Add write/read sessions and consistent backup

**Files:**
- Create: `src-tauri/src/persistence/write_coordinator.rs`
- Create: `src-tauri/src/persistence/write_session.rs`
- Create: `src-tauri/src/persistence/read_session.rs`
- Create: `src-tauri/src/persistence/backup.rs`
- Create: `src-tauri/tests/persistence_sessions.rs`
- Modify: `src-tauri/src/persistence/mod.rs`
- Modify: `src-tauri/src/persistence/migrations.rs`

- [ ] **Step 1: Write failing fairness, cancellation, snapshot, and backup tests**

```rust
#[tokio::test]
async fn cancelled_uncommitted_write_rolls_back_and_releases_permit() {
    let runtime = TestRuntime::open().await;
    let store = TestStore::default();
    let task = tokio::spawn(runtime.write(|session| async move {
        store.insert(session, "cancelled").await?;
        std::future::pending::<()>().await;
        Ok(())
    }));
    task.abort();
    task.await.expect_err("aborted");
    assert_eq!(TestStore::default().row_count(&runtime).await, 0);
    runtime.write(|_| async { Ok(()) }).await.expect("permit released");
}

#[tokio::test]
async fn read_session_keeps_one_snapshot_across_two_queries() {
    let runtime = TestRuntime::open().await;
    let store = TestStore::default();
    let mut read = runtime.begin_read().await.unwrap();
    let before = store.value(&mut read).await.unwrap();
    store.replace(&runtime, "new").await.unwrap();
    let same_snapshot = store.value(&mut read).await.unwrap();
    assert_eq!(same_snapshot, before);
    assert_eq!(store.value(&mut runtime.begin_read().await.unwrap()).await.unwrap(), "new");
}
```

`TestStore` exists only in `src-tauri/tests/persistence_sessions.rs`; do not add test methods to production sessions. Backup tests must enable WAL, insert committed WAL data, run backup to a generated temporary destination, reopen/quick-check it, and prove interrupted temporary output never becomes a final recovery candidate.

Add this future-migration regression after the backup fixture exists:

```rust
#[tokio::test]
async fn pending_v2_migration_does_not_start_when_verified_backup_fails() {
    let fixture = V2Fixture::with_pending_migration().await;
    fixture.inject_backup_failure();
    let before = fixture.database_hash();
    let error = fixture.run_pending_migrations().await.unwrap_err();
    assert!(matches!(error, PersistenceError::IoFailed));
    assert_eq!(fixture.database_hash(), before);
    assert!(!fixture.runtime_registered());
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_sessions -- --nocapture
```

Expected: compile failure for missing coordinator/session/backup APIs.

- [ ] **Step 3: Implement opaque sessions and commit uncertainty**

```rust
pub(crate) struct WriteSession<'a> {
    transaction: sqlx::Transaction<'a, sqlx::Sqlite>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

pub(crate) struct ReadSession<'a> {
    transaction: sqlx::Transaction<'a, sqlx::Sqlite>,
}
```

Add `writes: Arc<WriteCoordinator>` to `PersistenceHandle` and expose only `begin_read`, `write`, health, and shutdown methods to persistence/application callers. Use one fair Tokio semaphore permit for writes. No Store may acquire another connection while it holds `&mut WriteSession`. A dropped pre-commit session rolls back; a fault after commit dispatch maps to `CommitOutcomeUnknown` and must be reconciled by idempotency key/state precondition.

Runtime health keeps bounded counters/histograms for pool acquire, writer queue wait, transaction duration/outcome, fixed query category, busy/locked retry, and shutdown drain. No metric label may contain SQL, path, station/key ID, URL, or request body.

- [ ] **Step 4: Implement one backup path**

Execute parameterized `VACUUM INTO` outside a transaction on a dedicated acquired connection. Require a non-existing app-generated temporary destination, `sync_all`, read-only reopen, `quick_check`, atomic final rename, and parent-directory sync. Do not copy the SQLite main file and do not add a second online-backup implementation.

Update `migrations.rs` so pending V2 migrations run only in this order: current health/compatibility inspection, verified backup, transactional SQLx migrations, compatibility-row validation, read-only reopen, `quick_check`/foreign-key/projection validation, then writable runtime registration. A failed backup or validation leaves the original V2 database authoritative and unchanged.

- [ ] **Step 5: Run concurrency/fault tests and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_sessions -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml persistence:: -- --nocapture
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
git add -- src-tauri/src/persistence/write_coordinator.rs src-tauri/src/persistence/write_session.rs src-tauri/src/persistence/read_session.rs src-tauri/src/persistence/backup.rs src-tauri/src/persistence/runtime.rs src-tauri/src/persistence/migrations.rs src-tauri/src/persistence/mod.rs src-tauri/tests/persistence_sessions.rs
git commit -m "feat: add bounded persistence sessions"
```

## Workstream B: Vertical Store And Application Migration

### Task 5: Create the differential harness and migrate Station/Settings

**Files:**
- Create: `src-tauri/src/persistence/migrations/0002_catalog_settings.sql`
- Create: `src-tauri/src/persistence/stores/mod.rs`
- Create: `src-tauri/src/persistence/stores/station_catalog.rs`
- Create: `src-tauri/src/persistence/stores/settings_store.rs`
- Create: `src-tauri/src/application/stations.rs`
- Create: `src-tauri/src/application/settings.rs`
- Create: `src-tauri/src/commands/stations.rs`
- Create: `src-tauri/src/commands/settings.rs`
- Create: `src-tauri/src/persistence/differential_tests.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/persistence/mod.rs`
- Modify: `src-tauri/src/application/mod.rs`

- [ ] **Step 1: Write failing V1/V2 station and settings contracts**

Use separate cloned databases; never call V1 and V2 writers against one file:

```rust
#[tokio::test]
async fn station_endpoint_change_is_atomic_and_matches_v1_contract() {
    let fixture = ReleasedFixture::v031();
    let v1 = fixture.clone_for_v1();
    let v2 = fixture.import_empty_v2().await;
    let input = fixture.endpoint_change_input();

    let expected = run_v1_station_endpoint_change(&v1, input.clone());
    let actual = StationService::new(v2.runtime()).update_station(input).await;

    assert_eq!(actual.unwrap().sanitized_manifest(), expected.unwrap().sanitized_manifest());
    assert_eq!(v2.station_manifest().await, fixture.expected_endpoint_change_manifest());
}

#[tokio::test]
async fn settings_are_typed_and_unknown_values_do_not_enter_v2() {
    let runtime = V2Fixture::with_legacy_settings([
        ("proxy_port", "8787"),
        ("retired_secret_setting", "canary"),
    ]).await;
    let settings = SettingsService::new(runtime.handle()).load().await.unwrap();
    assert_eq!(settings.proxy_port, 8787);
    assert!(!runtime.any_text_contains("canary").await);
}
```

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests station_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests settings_ -- --nocapture
```

Expected: compile failure for missing migrations, stores, and services.

- [ ] **Step 3: Implement the consistency-oriented APIs**

```rust
pub(crate) struct StationCatalogStore;

impl StationCatalogStore {
    pub(crate) async fn list(&self, read: &mut ReadSession<'_>) -> Result<Vec<Station>, PersistenceError>;
    pub(crate) async fn insert(&self, write: &mut WriteSession<'_>, station: NewStationRow) -> Result<Station, PersistenceError>;
    pub(crate) async fn update_if_revision(&self, write: &mut WriteSession<'_>, change: StationChange) -> Result<Station, PersistenceError>;
    pub(crate) async fn delete_owned_state(&self, write: &mut WriteSession<'_>, station_id: &str) -> Result<(), PersistenceError>;
}

pub(crate) struct StationService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}
```

`StationService::update_station` owns one write transaction containing endpoint revision, health cleanup, credential/session invalidation, and enabled-state changes. `SettingsService` owns typed defaults/validation; `SettingsStore` persists only supported keys and contains no string-to-business-policy fallbacks.

- [ ] **Step 4: Move SQL without changing semantics**

Move the explicit station/settings SQL and private row mapping from these `AppDatabase` symbols into the two stores: `list_stations`, `create_station_with_data_key`, `update_station_with_data_key`, `with_station_endpoint_revision`, `delete_station`, `reorder_stations`, `get_settings`, `ensure_secure_local_access_key`, `update_local_access_key`, and `update_settings`. Keep V1 methods intact for differential fixtures; production commands remain on V1 until Task 14.

- [ ] **Step 5: Run GREEN and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests station_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests settings_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_runtime -- --nocapture
powershell -ExecutionPolicy Bypass -File scripts/prepare-sqlx.ps1
cargo test --manifest-path src-tauri/Cargo.toml persistence::stores::station_catalog -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/persistence/migrations/0002_catalog_settings.sql src-tauri/src/persistence/stores/mod.rs src-tauri/src/persistence/stores/station_catalog.rs src-tauri/src/persistence/stores/settings_store.rs src-tauri/src/application/stations.rs src-tauri/src/application/settings.rs src-tauri/src/commands/stations.rs src-tauri/src/commands/settings.rs src-tauri/src/commands/mod.rs src-tauri/src/persistence/mod.rs src-tauri/src/application/mod.rs src-tauri/src/persistence/differential_tests.rs .sqlx
git commit -m "feat: migrate station and settings persistence"
```

### Task 6: Migrate credentials, Station Keys, and remote-key bindings

**Files:**
- Create: `src-tauri/src/persistence/migrations/0003_credentials_keys.sql`
- Create: `src-tauri/src/persistence/stores/credential_store.rs`
- Create: `src-tauri/src/application/credentials.rs`
- Create: `src-tauri/src/commands/credentials.rs`
- Modify: `src-tauri/src/persistence/stores/station_catalog.rs`
- Modify: `src-tauri/src/persistence/differential_tests.rs`
- Modify: `src-tauri/src/services/secrets/**`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`

- [ ] **Step 1: Write failing secret/AAD and key lifecycle tests**

```rust
#[tokio::test]
async fn secret_replacement_commits_ciphertext_and_reference_atomically() {
    let vault = DeterministicCredentialVault::new([7; 32]);
    let runtime = V2Fixture::open().await;
    let service = CredentialService::new(runtime.handle(), vault.clone());

    let saved = service.replace_station_key_secret("station-1", "key-1", SecretBytes::from(b"sk-test".to_vec())).await.unwrap();

    assert_eq!(saved.secret_ref.owner_id, "key-1");
    assert_eq!(vault.last_aad(), "station_key:key-1:api_key");
    assert!(!runtime.any_text_contains("sk-test").await);
}
```

Also cover blank update preserving ciphertext, endpoint-revision stale session rejection, Station Key ordering, remote-key binding station scope, delete cascades, and deterministic imported IDs.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests credential_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests station_key_ -- --nocapture
```

- [ ] **Step 3: Implement the vault boundary and one transaction owner**

```rust
pub(crate) struct SecretBytes(zeroize::Zeroizing<Vec<u8>>);

impl From<Vec<u8>> for SecretBytes {
    fn from(value: Vec<u8>) -> Self {
        Self(zeroize::Zeroizing::new(value))
    }
}

pub(crate) trait CredentialVault: Send + Sync {
    fn encrypt(&self, aad: &str, plaintext: SecretBytes) -> Result<EncryptedSecret, CredentialError>;
    fn decrypt(&self, aad: &str, encrypted: &EncryptedSecret) -> Result<SecretBytes, CredentialError>;
}

pub(crate) struct CredentialService {
    runtime: PersistenceHandle,
    vault: Arc<dyn CredentialVault>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
}
```

Add `zeroize = "1"` to production dependencies. Tests may reveal only fixed canary equality through a test-only vault; production errors and Debug output never expose `SecretBytes`.

Plaintext may exist only as `SecretBytes` inside Vault/Application scope; it must not enter IPC DTO, Store row, error, log, or `String` helper. Preserve AAD format `"{scope}:{owner_id}:{kind}"` exactly.

- [ ] **Step 4: Move the owned SQL set**

Move `get/update/clear_station_credentials`, session persistence/invalidation, Station Key create/update/delete/reorder, capabilities, remote-key replace/bind/unbind, and secret-reference SQL into `CredentialStore` or `StationCatalogStore` according to the Task 0 field ledger. A save-with-defaults use case owns one transaction across key, capability defaults, group binding, and ordering.

- [ ] **Step 5: Run security/differential gates and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests credential_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests station_key_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml secrets:: -- --nocapture
node scripts/data-store-diagnostic-redaction.test.mjs
powershell -ExecutionPolicy Bypass -File scripts/prepare-sqlx.ps1
git add -- src-tauri/src/persistence/migrations/0003_credentials_keys.sql src-tauri/src/persistence/stores/credential_store.rs src-tauri/src/persistence/stores/station_catalog.rs src-tauri/src/application/credentials.rs src-tauri/src/commands/credentials.rs src-tauri/src/persistence/differential_tests.rs src-tauri/Cargo.toml src-tauri/Cargo.lock .sqlx
git add -p -- src-tauri/src/services/secrets
git commit -m "feat: migrate credential and key persistence"
```

### Task 7: Migrate routing facts and workflow queries

**Files:**
- Create: `src-tauri/src/persistence/migrations/0004_routing.sql`
- Create: `src-tauri/src/persistence/stores/routing_store.rs`
- Create: `src-tauri/src/application/routing.rs`
- Create: `src-tauri/src/application/queries/station_assets.rs`
- Create: `src-tauri/src/application/queries/station_detail.rs`
- Create: `src-tauri/src/commands/routing.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/persistence/differential_tests.rs`

- [ ] **Step 1: Write failing canonical routing differential tests**

```rust
#[tokio::test]
async fn routing_candidates_preserve_identity_order_and_eligibility() {
    let fixture = ReleasedFixture::routing_matrix();
    let expected = fixture.v1_routing_manifest();
    let v2 = fixture.import_empty_v2().await;
    let actual = RoutingService::new(v2.runtime()).load_candidates(
        RoutingQuery { model: Some("gpt-5".into()), group: None, now_ms: 1_000 },
    ).await.unwrap();
    assert_eq!(actual.sanitized_manifest(), expected);
}
```

Cover group binding identity, category aliases, endpoint revision, missing/disabled binding, station-scope balance precedence, model aliases, manual priority/order, cooldown, and pricing facts used by routing.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests routing_ -- --nocapture
```

- [ ] **Step 3: Implement purpose-built routing reads**

```rust
pub(crate) struct RoutingStore;

impl RoutingStore {
    pub(crate) async fn load_schedulable_candidates(
        &self,
        read: &mut ReadSession<'_>,
        query: &RoutingQuery,
    ) -> Result<Vec<SchedulerCandidate>, PersistenceError>;
    pub(crate) async fn apply_health_feedback(
        &self,
        write: &mut WriteSession<'_>,
        feedback: HealthFeedback,
    ) -> Result<(), PersistenceError>;
}
```

Use explicit columns and deterministic ordering with stable ID tie-breakers. Do not copy scheduler/scoring policy into SQL or Row mapping. Query services return named bounded read models and use one `ReadSession` only when multiple statements are unavoidable.

- [ ] **Step 4: Adapt the consumer-owned port**

Keep the `RoutingRepository` trait owned by `services/proxy`. Add a V2 adapter that calls `RoutingService` and exercise it in integration composition; the proxy module must not import `RoutingStore`, SQLx, Pool, or `PersistenceHandle`. Keep the current V1 adapter as the registered production adapter until Task 14, then retain it only for released-fixture differential support until Task 16.

- [ ] **Step 5: Run routing gates and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests routing_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::proxy::scheduler -- --nocapture
node scripts/local-routing-query-service.test.mjs
node scripts/routing-query-service.test.mjs
powershell -ExecutionPolicy Bypass -File scripts/prepare-sqlx.ps1
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/persistence/migrations/0004_routing.sql src-tauri/src/persistence/stores/routing_store.rs src-tauri/src/application/routing.rs src-tauri/src/application/queries/station_assets.rs src-tauri/src/application/queries/station_detail.rs src-tauri/src/commands/routing.rs src-tauri/src/services/proxy/routing_repository.rs src-tauri/src/persistence/differential_tests.rs .sqlx
git commit -m "feat: migrate routing persistence"
```

### Task 8: Replace Proxy finalization persistence safely

**Files:**
- Create: `src-tauri/src/persistence/migrations/0005_request_logs.sql`
- Create: `src-tauri/src/persistence/stores/request_log_store.rs`
- Create: `src-tauri/src/application/request_finalization.rs`
- Modify: `src-tauri/src/services/proxy/response_body.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/persistence/differential_tests.rs`

- [ ] **Step 1: Write failing bounded finalization tests**

```rust
#[tokio::test]
async fn duplicate_finalization_inserts_once_and_updates_health_once() {
    let runtime = V2Fixture::open().await;
    let service = Arc::new(RequestFinalizationService::new(runtime.handle()));
    let port: Arc<dyn RequestFinalizationPort> = Arc::new(V2FinalizationAdapter::new(service));
    let dispatcher = FinalizationDispatcher::new(2, port);
    let outcome = success_outcome("request-1");

    dispatcher.reserve().unwrap().finalize(outcome.clone());
    dispatcher.reserve().unwrap().finalize(outcome);
    dispatcher.shutdown(Duration::from_secs(1)).await.unwrap();

    assert_eq!(runtime.request_log_count("request-1").await, 1);
    assert_eq!(runtime.health_feedback_count("request-1").await, 1);
}
```

Add EOF, body error, idle timeout, drop-before-poll, drop-after-chunk, capacity exhaustion before upstream execution, transient persistence retry, permanent failure unhealthy, and shutdown drain-order tests.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::proxy::response_body -- --nocapture
```

Expected: existing tests expose ignored persistence errors, separate semaphore/channel reservation, detached fallback sends, or missing owned shutdown result.

- [ ] **Step 3: Make the channel permit the only admission reservation**

```rust
pub(crate) struct FinalizationLease {
    permit: Option<tokio::sync::mpsc::OwnedPermit<FinalizationJob>>,
    finalized: bool,
}

pub(crate) struct FinalizationDispatcher {
    sender: Option<tokio::sync::mpsc::Sender<FinalizationJob>>,
    worker: tokio::task::JoinHandle<Result<(), FinalizationWorkerError>>,
    health: Arc<DispatcherHealth>,
}

pub(crate) type FinalizationFuture = Pin<
    Box<dyn Future<Output = Result<(), FinalizationWorkerError>> + Send + 'static>,
>;

pub(crate) trait RequestFinalizationPort: Send + Sync {
    fn finalize(&self, outcome: FinalRequestOutcome) -> FinalizationFuture;
}
```

Reserve the actual channel slot before upstream execution. `finalize` consumes `OwnedPermit::send` synchronously, including Drop paths. Delete per-job fallback spawn and the permanent request-ID `HashSet`.

Expose bounded finalization metrics: admission saturation, queue depth, retry count, oldest-job age, worker health, and shutdown outcome. Labels are fixed enums only.

- [ ] **Step 4: Make the database constraint authoritative**

`RequestFinalizationPort` is owned by Proxy. A temporary V1 adapter and the V2 application adapter implement it; Task 14 swaps registration and Task 16 deletes V1. `request_id` is non-null and unique. `RequestFinalizationService` owns one write transaction that performs endpoint-revision validation, `INSERT ... ON CONFLICT(request_id) DO NOTHING`, checks `rows_affected`, and writes health feedback only when one row was inserted. Worker errors remain owned until persisted or returned as typed shutdown failure; they are never assigned to `_`.

- [ ] **Step 5: Run Proxy and differential gates, then commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::proxy::response_body -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::proxy::runtime -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests request_finalization_ -- --nocapture
node scripts/proxy-lifecycle-contract.test.mjs
node scripts/local-proxy-v2-boundary.test.mjs
powershell -ExecutionPolicy Bypass -File scripts/prepare-sqlx.ps1
git add -- src-tauri/src/persistence/migrations/0005_request_logs.sql src-tauri/src/persistence/stores/request_log_store.rs src-tauri/src/application/request_finalization.rs src-tauri/src/services/proxy/response_body.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/routing_repository.rs src-tauri/src/persistence/differential_tests.rs .sqlx
git commit -m "feat: harden proxy finalization persistence"
```

### Task 9: Migrate Collector Apply and change-event lifecycle

**Files:**
- Create: `src-tauri/src/persistence/migrations/0006_collectors_changes.sql`
- Create: `src-tauri/src/persistence/stores/collector_store.rs`
- Create: `src-tauri/src/persistence/stores/change_store.rs`
- Create: `src-tauri/src/application/collectors.rs`
- Create: `src-tauri/src/commands/collectors.rs`
- Modify: `src-tauri/src/services/collectors/apply.rs`
- Modify: `src-tauri/src/services/station_collectors.rs`
- Modify: `src-tauri/src/persistence/differential_tests.rs`

- [ ] **Step 1: Write failing all-or-nothing collector tests**

```rust
#[tokio::test]
async fn collector_apply_rolls_back_snapshot_facts_events_and_run_on_failure() {
    let runtime = V2Fixture::open().await;
    runtime.inject_failure(WriteFault::BeforeCollectorRunFinish);
    let service = CollectorService::new(runtime.handle());

    let error = service.apply_result(full_collector_result()).await.unwrap_err();

    assert!(matches!(error, ApplicationError::Internal));
    assert_eq!(runtime.collector_manifest().await, CollectorManifest::empty());
}
```

Cover parent/child run completion, station/task-scoped failure and recovery, read/dismissed state preservation, group added/missing/rate change identities, endpoint revision stale rejection, and no event from unsupported provider facts.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests collector_ -- --nocapture
```

- [ ] **Step 3: Implement one application-owned transaction**

`CollectorService::apply_result` obtains one `WriteSession` and passes it to `CollectorStore` and `ChangeStore`. Provider adapters output canonical facts only; they receive neither pool nor transaction. Add a collector-owned V2 apply port adapter for integration composition; keep the V1 adapter registered until Task 14. Move snapshot, current fact, run, due scheduling, failure/recovery, and event SQL from `AppDatabase` into the two stores without moving provider parsers.

- [ ] **Step 4: Run collector gates and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests collector_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::collectors -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::station_collectors -- --nocapture
node scripts/change-center-collector-task-label.test.mjs
node scripts/newapi-collector-contract.test.mjs
powershell -ExecutionPolicy Bypass -File scripts/prepare-sqlx.ps1
git add -- src-tauri/src/persistence/migrations/0006_collectors_changes.sql src-tauri/src/persistence/stores/collector_store.rs src-tauri/src/persistence/stores/change_store.rs src-tauri/src/application/collectors.rs src-tauri/src/commands/collectors.rs src-tauri/src/services/collectors/apply.rs src-tauri/src/services/station_collectors.rs src-tauri/src/persistence/differential_tests.rs .sqlx
git commit -m "feat: migrate collector persistence"
```

### Task 10: Migrate pricing, monitoring, and bounded query projections

**Files:**
- Create: `src-tauri/src/persistence/migrations/0007_pricing_monitoring.sql`
- Create: `src-tauri/src/persistence/stores/pricing_store.rs`
- Create: `src-tauri/src/persistence/stores/monitoring_store.rs`
- Create: `src-tauri/src/application/pricing.rs`
- Create: `src-tauri/src/application/monitoring.rs`
- Create: `src-tauri/src/application/queries/pricing_comparison.rs`
- Create: `src-tauri/src/application/queries/channel_status.rs`
- Create: `src-tauri/src/application/queries/change_center.rs`
- Create: `src-tauri/src/commands/pricing.rs`
- Create: `src-tauri/src/commands/monitoring.rs`
- Modify: `src-tauri/src/services/channel_monitors/mod.rs`
- Modify: `src-tauri/src/persistence/differential_tests.rs`

- [ ] **Step 1: Write failing projection and lifecycle tests**

```rust
#[tokio::test]
async fn pricing_workspace_is_bounded_deterministic_and_matches_v1() {
    let fixture = ReleasedFixture::pricing_and_monitoring();
    let expected = fixture.v1_pricing_workspace_manifest();
    let v2 = fixture.import_empty_v2().await;
    let actual = PricingComparisonQuery::new(v2.runtime()).load(PageLimit::new(200).unwrap()).await.unwrap();
    assert_eq!(actual.sanitized_manifest(), expected);
}
```

Cover latest station-scope balance, group-rate identity, model price/rule selection, channel monitor template CRUD, due scheduling, monitor run validation, channel status rollup, change-center pagination, and deterministic tie-breakers.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests pricing_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests monitoring_ -- --nocapture
```

- [ ] **Step 3: Implement purpose-built stores and queries**

Define one bounded pagination value and reject zero/oversized input at the application boundary:

```rust
#[derive(Debug, Clone, Copy)]
pub(crate) struct PageLimit(u32);

impl PageLimit {
    pub(crate) fn new(value: u32) -> Result<Self, ApplicationError> {
        (1..=500).contains(&value).then_some(Self(value)).ok_or(ApplicationError::ConstraintViolation)
    }
    pub(crate) fn get(self) -> u32 { self.0 }
}
```

Growth tables expose only cursor/page-limited reads. Stable high-frequency workspaces use one explicit projection SQL where possible; multi-query workspaces use one short `ReadSession`. Monitoring runner adapters compile against the V2 service in integration composition but remain unregistered until Task 14. Do not return generic maps or copy pricing/monitor policy into Row mapping.

- [ ] **Step 4: Run query plans and consumer gates**

Run `EXPLAIN QUERY PLAN` assertions for request log, pricing comparison, channel status, change center, and latest balance indexes, then:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests pricing_ -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests monitoring_ -- --nocapture
node scripts/pricing-facts-projection.test.mjs
node scripts/channel-status-backend-rollup-contract.test.mjs
node scripts/change-query-service.test.mjs
node scripts/request-log-pagination.test.mjs
powershell -ExecutionPolicy Bypass -File scripts/prepare-sqlx.ps1
```

- [ ] **Step 5: Commit the final business slice**

```powershell
git add -- src-tauri/src/persistence/migrations/0007_pricing_monitoring.sql src-tauri/src/persistence/stores/pricing_store.rs src-tauri/src/persistence/stores/monitoring_store.rs src-tauri/src/application/pricing.rs src-tauri/src/application/monitoring.rs src-tauri/src/application/queries/station_assets.rs src-tauri/src/application/queries/station_detail.rs src-tauri/src/application/queries/pricing_comparison.rs src-tauri/src/application/queries/channel_status.rs src-tauri/src/application/queries/change_center.rs src-tauri/src/commands/pricing.rs src-tauri/src/commands/monitoring.rs src-tauri/src/services/channel_monitors/mod.rs src-tauri/src/persistence/differential_tests.rs .sqlx
git commit -m "feat: migrate pricing and monitoring persistence"
```

### Task 11: Compose AppServices and prove all production consumers have owners

**Files:**
- Create: `src-tauri/src/application/app_services.rs`
- Modify: `src-tauri/src/application/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `docs/superpowers/audits/persistence-v2-boundary-manifest.json`
- Modify: `src-tauri/tests/persistence_architecture.rs`

- [ ] **Step 1: Write the failing composition-boundary test**

```rust
#[test]
fn app_services_is_only_used_by_composition_and_command_adapters() {
    let graph = ParsedModuleGraph::load("src").unwrap();
    let manifest = BoundaryManifest::load(
        "../docs/superpowers/audits/persistence-v2-boundary-manifest.json",
    ).unwrap();
    assert_eq!(
        graph.consumers_of("application::app_services::AppServices"),
        BTreeSet::from(["lib".to_string(), "commands".to_string()]),
    );
    assert_eq!(
        graph.production_consumers_of("services::database::AppDatabase"),
        manifest.temporary_legacy_consumers.clone(),
    );
}
```

The manifest's `temporary_legacy_consumers` list is frozen from Task 0 and may only shrink. Task 14 makes it empty during production cutover; Task 16 deletes the symbol.

- [ ] **Step 2: Implement composition without a service locator**

```rust
pub(crate) struct AppServices {
    pub stations: Arc<StationService>,
    pub credentials: Arc<CredentialService>,
    pub collectors: Arc<CollectorService>,
    pub routing: Arc<RoutingService>,
    pub request_finalization: Arc<RequestFinalizationService>,
    pub monitoring: Arc<MonitoringService>,
    pub pricing: Arc<PricingService>,
    pub settings: Arc<SettingsService>,
}
```

`AppServices` has a constructor and fields only. Application services receive concrete narrow dependencies, never `AppServices`. Commands perform ACL/DTO conversion/service call/error mapping only.

- [ ] **Step 3: Classify every remaining production consumer**

Use the boundary manifest as a checklist. Every remaining `AppDatabase` consumer must name the V2 service/query/consumer port that replaces it and deletion owner Task 14 or 16. Remove any consumer already migrated, forbid additions, and stop for design review if a consumer has no owner rather than adding a catch-all method. Tests may retain V1 only inside released-fixture differential support.

- [ ] **Step 4: Run architecture and contract gates, then commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests -- --nocapture
pnpm.cmd test:contracts
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/application/app_services.rs src-tauri/src/application/mod.rs src-tauri/src/commands src-tauri/src/lib.rs docs/superpowers/audits/persistence-v2-boundary-manifest.json src-tauri/tests/persistence_architecture.rs
git commit -m "refactor: compose persistence v2 application services"
```

## Workstream C: Generation Upgrade And Cutover

### Task 12: Build released-schema fixtures and the read-only importer

**Files:**
- Create: `scripts/build-persistence-v2-fixtures.ps1`
- Create: `src-tauri/src/persistence/legacy_import/detect.rs`
- Create: `src-tauri/src/persistence/legacy_import/import.rs`
- Create: `src-tauri/src/persistence/legacy_import/validate.rs`
- Create: `src-tauri/src/persistence/legacy_import/profiles/mod.rs`
- Create: `src-tauri/src/persistence/legacy_import/profiles/profile_001.rs`
- Create: additional `profile_NNN.rs` files only for distinct released schema hashes recorded in Task 0
- Create: `src-tauri/tests/persistence_upgrade.rs`
- Create: `src-tauri/tests/persistence_upgrade/fixtures/profile_NNN/{source.sqlite3,expected_manifest.json}`

- [ ] **Step 1: Generate sanitized fixtures from every public release**

The PowerShell script checks out each tag into a temporary worktree outside `src-tauri`, creates a database through that release's normal initialization path, inserts deterministic non-secret canaries, closes the process, copies main/WAL/SHM as one fixture set, and removes the temporary worktree. It must compute schema hashes from ordered `sqlite_schema` rows and group tags with identical hashes into one profile.

Run:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build-persistence-v2-fixtures.ps1 -Tags @('v0.3.1')
git diff -- src-tauri/tests/persistence_upgrade/fixtures docs/superpowers/audits/persistence-v2-released-schema-manifest.json
```

Expected: `v0.3.1` maps to exactly one profile; earlier tags are recorded as unsupported and do not generate importer profiles. Fixtures contain no key/cookie/token/password or absolute user path.

- [ ] **Step 2: Write failing detect/import/manifest tests**

```rust
#[tokio::test]
async fn every_released_profile_imports_to_the_expected_manifest() {
    for fixture in released_fixtures() {
        let profile = detect_profile(fixture.source_path()).await.expect("known profile");
        let target = V2Fixture::new_target().await;
        import_profile(&profile, fixture.source_path(), target.runtime()).await.unwrap();
        validate_import(target.runtime(), fixture.expected_manifest()).await.unwrap();
    }
}

#[tokio::test]
async fn unknown_future_schema_fails_without_touching_source() {
    let fixture = unknown_schema_fixture();
    let before = fixture.file_set_hash();
    assert!(matches!(detect_profile(fixture.source_path()).await, Err(UpgradeError::UnsupportedLegacySchema)));
    assert_eq!(fixture.file_set_hash(), before);
}
```

- [ ] **Step 3: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture
```

- [ ] **Step 4: Implement one profile per schema shape**

```rust
type ImportFuture<'a> = Pin<Box<dyn Future<Output = Result<(), UpgradeError>> + Send + 'a>>;

pub(crate) struct LegacyProfileDescriptor {
    pub id: &'static str,
    pub schema_hash: &'static str,
    pub import: for<'a> fn(
        &'a mut LegacyReadSession<'a>,
        &'a PersistenceHandle,
    ) -> ImportFuture<'a>,
}

pub(crate) struct DetectedLegacyProfile {
    descriptor: &'static LegacyProfileDescriptor,
}
```

`profiles::all() -> &'static [LegacyProfileDescriptor]` is an explicit compile-time slice containing every distinct hash in the Task 0 released manifest. It has no runtime registration, wildcard, provider hook, or dynamic loading. `detect_profile` selects by schema hash; each descriptor delegates to one focused profile module. Each profile uses explicit legacy column names and typed conversions, preserves canonical IDs/revisions/statuses, generates deterministic importer-only request IDs from profile + legacy primary key, and sends plaintext secrets directly through `CredentialVault` into V2 ciphertext. No profile writes the source or returns plaintext DTOs.

- [ ] **Step 5: Validate full import order and source immutability**

Import settings/install metadata, stations/revisions, encrypted secrets/references, keys/capabilities, group/routing/model aliases, monitors, pricing, evidence/history, health/change events, then derived projections/indexes. On any failure discard the target temporary DB and prove source main/WAL/SHM hashes and mtimes are unchanged.

- [ ] **Step 6: Run security and matrix gates, then commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests -- --nocapture
node scripts/data-store-upgrade-matrix.test.mjs
git add -- scripts/build-persistence-v2-fixtures.ps1 src-tauri/src/persistence/legacy_import src-tauri/tests/persistence_upgrade.rs src-tauri/tests/persistence_upgrade/fixtures docs/superpowers/audits/persistence-v2-released-schema-manifest.json
git commit -m "feat: import released databases into persistence v2"
```

### Task 13: Implement UpgradeJournal, RecoveryPlanner, executor, and tombstone

**Files:**
- Create: `src-tauri/src/persistence/upgrade_journal.rs`
- Create: `src-tauri/src/persistence/upgrade_recovery_plan.rs`
- Create: `src-tauri/src/persistence/upgrade_recovery_executor.rs`
- Create: `src-tauri/tests/persistence_upgrade_recovery.rs`
- Modify: `src-tauri/src/services/data_store/config.rs`
- Modify: `src-tauri/src/services/data_store/inspect.rs`

- [ ] **Step 1: Write the exhaustive pure-planner tests**

```rust
#[test]
fn every_observed_upgrade_state_has_one_plan_or_halt() {
    for state in ObservedUpgradeState::finite_test_matrix() {
        let first = RecoveryPlanner::plan(state.clone());
        let second = RecoveryPlanner::plan(state.clone());
        assert_eq!(first, second, "planner must be deterministic for {state:?}");
        assert!(first.is_executable() || matches!(first, RecoveryPlan::Halt(_)));
    }
}

#[test]
fn changed_precondition_stops_destructive_execution() {
    let observed = observed_v2_validated();
    let plan = RecoveryPlanner::plan(observed.clone());
    let io = FakeUpgradeIo::from(observed).with_changed_backup_hash();
    assert_eq!(execute_plan(&io, plan), Err(UpgradeError::RecoveryPreconditionChanged));
    assert_eq!(io.destructive_calls(), 0);
}
```

The finite matrix includes all six phases, config generation 1/2, source generation/tombstone/missing/unknown, valid/invalid backup, temp/final candidate, relocation intent, compatibility result, and orphan artifacts.

- [ ] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture
```

- [ ] **Step 3: Implement closed journal and plan enums**

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum UpgradePhase {
    Prepared,
    BackupVerified,
    V2Validated,
    LegacyDeactivated,
    GenerationCommitted,
    V2Reopened,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecoveryPlan {
    RestartFromSource,
    RebuildV2FromVerifiedBackup,
    ActivateValidatedV2,
    RestoreGeneration1,
    CommitGeneration2,
    ReopenGeneration2,
    CleanupCompletedJournal,
    Halt(RecoveryHaltReason),
}

impl RecoveryPlan {
    pub(crate) fn is_executable(&self) -> bool {
        !matches!(self, Self::Halt(_))
    }
}
```

Journal payload contains version, attempt ID, phase, source/profile identity, verified backup SHA-256, allowlisted relative paths, timestamps, and canonical payload checksum only. Persist every phase by same-directory temporary write, `sync_all`, atomic replace, and parent sync.

- [ ] **Step 4: Implement observation and single-plan execution**

Observer performs read-only fact collection. Planner contains all phase decisions and no I/O. Executor revalidates identity/hash/precondition before each mutation and never recomputes a plan. Relocation intent and upgrade journal are mutually exclusive; unlisted combinations return `Halt` without cleanup.

Emit only recovery phase, plan kind, precondition outcome, and halt reason. Never emit artifact paths, hashes, journal payload, schema SQL, or imported values.

- [ ] **Step 5: Implement atomic tombstone replacement**

Write a same-directory temporary tombstone containing fixed magic, format version, attempt ID, and checksum; flush it; atomically replace the legacy main filename; sync parent; remove WAL/SHM; re-read magic; then persist `LegacyDeactivated`. Never expose a missing legacy main filename. V2 inspection recognizes tombstones; older binaries must receive `not a database`.

- [ ] **Step 6: Run crash-before/after-sync matrix and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture
node scripts/data-store-startup-boundary.test.mjs
git add -- src-tauri/src/persistence/upgrade_journal.rs src-tauri/src/persistence/upgrade_recovery_plan.rs src-tauri/src/persistence/upgrade_recovery_executor.rs src-tauri/tests/persistence_upgrade_recovery.rs src-tauri/src/services/data_store/config.rs src-tauri/src/services/data_store/inspect.rs
git commit -m "feat: add deterministic persistence upgrade recovery"
```

### Task 14: Cut startup, config, commands, and recovery UI to generation 2

**Implementation status (2026-07-21):** Production startup, commands, consumer ports, config V3, generation-2 reopen, recovery planner/executor, relocation exclusion, and runtime registration have been cut over. Focused startup/recovery/architecture tests pass. Signed installer behavior remains Task 17.

**Files:**
- Modify: `src-tauri/src/services/data_store/config.rs`
- Modify: `src-tauri/src/services/data_store/mod.rs`
- Modify: `src-tauri/src/services/data_store/decision.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/data_recovery.rs`
- Modify: `src-tauri/src/commands/stations.rs`
- Modify: `src-tauri/src/commands/settings.rs`
- Modify: `src-tauri/src/commands/credentials.rs`
- Modify: `src-tauri/src/commands/collectors.rs`
- Modify: `src-tauri/src/commands/routing.rs`
- Modify: `src-tauri/src/commands/pricing.rs`
- Modify: `src-tauri/src/commands/monitoring.rs`
- Modify: `src/lib/types/dataRecovery.ts`
- Modify: `src/lib/api/dataRecovery.ts`
- Modify: `src/features/data-recovery/DataStoreBootstrap.tsx`
- Modify: `src/features/data-recovery/DataRecoveryScreen.tsx`
- Modify: `src/features/data-recovery/recoveryViewModel.ts`
- Modify: related existing frontend tests in `src/features/data-recovery/*.test.ts*`
- Modify: `docs/superpowers/audits/persistence-v2-boundary-manifest.json`
- Modify: `src-tauri/tests/persistence_architecture.rs`
- Create: `src-tauri/tests/persistence_startup_cutover.rs`

- [x] **Step 1: Write failing config V2-to-V3 and startup tests**

```rust
#[test]
fn config_v2_upgrades_to_v3_without_losing_relocation_fields() {
    let v2 = DataDirConfigV2Fixture::with_active_pending_source();
    let v3 = DataDirConfigV3::try_from(v2.clone()).unwrap();
    assert_eq!(v3.database_generation, DatabaseGeneration::One);
    assert_eq!(v3.active_data_dir, v2.active_data_dir);
    assert_eq!(v3.pending_data_dir, v2.pending_data_dir);
    assert_eq!(v3.source_data_dir, v2.source_data_dir);
}

#[tokio::test]
async fn business_runtimes_register_only_after_v2_reopen_and_health() {
    let app = TestStartup::from_v031_fixture().await;
    app.run_upgrade().await.unwrap();
    assert_eq!(app.registration_trace(), ["lease", "upgrade", "v2_reopen", "health", "app_services", "proxy", "collectors", "monitors"]);
}
```

Also cover lock contention, inspection-only schema compatibility, pending relocation/upgrade mutual exclusion, config commit crash window, generation-2 reopen failure, and updater restart.

- [x] **Step 2: Run RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_startup_cutover -- --nocapture
pnpm.cmd exec vitest run src/features/data-recovery/DataStoreBootstrap.test.tsx src/features/data-recovery/recoveryViewModel.test.ts
```

- [x] **Step 3: Implement DataDirConfigV3 and startup order**

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum DatabaseGeneration { One, Two }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DataDirConfigV3 {
    pub version: u32,
    pub active_data_dir: Option<PathBuf>,
    pub pending_data_dir: Option<PathBuf>,
    pub source_data_dir: Option<PathBuf>,
    pub database_generation: DatabaseGeneration,
    pub updated_at: String,
}
```

Startup order is lease, config/relocation inspection, recovery planning, read-only legacy detection, backup/import/validation, tombstone, generation commit, V2 reopen/health/compatibility, AppServices registration, then Proxy/Collector/Monitor registration. A V2 config without generation maps deterministically to generation 1; invalid/unknown config never defaults to an empty DB.

- [x] **Step 4: Switch commands and consumer ports in one commit**

Register `AppServices`, stop registering `AppDatabase`, route every command module and Proxy/Collector/Monitor port to its application owner, and preserve public camelCase DTO/error payloads. Set `temporary_legacy_consumers` to an empty array and make the architecture test reject any production `AppDatabase` consumer; V1 remains reachable only from differential fixture code. Inspection-only mode exposes only backup, diagnostic, updater, and explicit recovery actions; the normal React app is not mounted.

- [ ] **Step 5: Run real startup/frontend gates and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_startup_cutover -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture
pnpm.cmd exec vitest run src/features/data-recovery/DataStoreBootstrap.test.tsx src/features/data-recovery/recoveryViewModel.test.ts
node scripts/data-store-startup-boundary.test.mjs
node scripts/native-shell-single-instance-tray.test.mjs
pnpm.cmd build
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/data_store/config.rs src-tauri/src/services/data_store/mod.rs src-tauri/src/services/data_store/decision.rs src-tauri/src/lib.rs src-tauri/src/commands/mod.rs src-tauri/src/commands/stations.rs src-tauri/src/commands/settings.rs src-tauri/src/commands/credentials.rs src-tauri/src/commands/collectors.rs src-tauri/src/commands/routing.rs src-tauri/src/commands/pricing.rs src-tauri/src/commands/monitoring.rs src-tauri/src/commands/data_recovery.rs src-tauri/tests/persistence_startup_cutover.rs src-tauri/tests/persistence_architecture.rs src/lib/types/dataRecovery.ts src/lib/api/dataRecovery.ts src/features/data-recovery/DataStoreBootstrap.tsx src/features/data-recovery/DataStoreBootstrap.test.tsx src/features/data-recovery/DataRecoveryScreen.tsx src/features/data-recovery/recoveryViewModel.ts src/features/data-recovery/recoveryViewModel.test.ts docs/superpowers/audits/persistence-v2-boundary-manifest.json
git commit -m "feat: cut startup over to persistence v2"
```

## Workstream D: Proof, Deletion, And Release

### Task 15: Run differential, fault, security, and performance qualification

**Implementation status (2026-07-22):** Production recovery fault injection, the atomic edge matrix, lifecycle drain, lease release, bounded write-queue observability, and real production-path performance/differential modules are implemented and have focused evidence. Proxy startup now injects `services.request_finalization` as `Arc<dyn RequestLifecycleStore>`; the runtime does not construct a concrete finalization service. The formal controlled-machine V1/V2 paired performance qualification is green and retained at `D:\Dev\Build\relay-pool-persistence-v2-qualification\paired-v0.3.1-v2.json`: hot-request-log p95 is 1.15 ms versus a reconstructed V1 p95 of 2.6345 ms, within the 10% gate; the other paired workloads also pass. Final contracts, frontend tests/build, SQLx offline metadata, and tracked artifact checks are green. Candidate-bundle evidence and signed-installer evidence belong to the remaining release gates, especially Task 17.

**Files:**
- Create: `src-tauri/tests/persistence_fault_matrix.rs`
- Create: `src-tauri/src/persistence/performance_tests.rs`
- Create: `src-tauri/src/persistence/differential_tests.rs`
- Create: `docs/superpowers/audits/2026-07-18-persistence-v2-qualification.md`
- Modify: `scripts/release-verification-entrypoint.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`

- [ ] **Step 1: Add the complete failure matrix**

The table-driven test injects failure at lease acquire/transfer, source open, integrity, backup, each import phase, compatibility validation, secret validation, file activation, every journal phase write, tombstone flush/replace, config commit, V2 reopen, service registration, finalization drain, runtime close, and lease release. Every row asserts authoritative generation, recovery plan, unchanged protected source/backup, and redacted diagnostic.

- [ ] **Step 2: Add the standard performance fixture**

Generate 100 stations, 1,000 Station Keys, 10,000 request logs, and 100,000 evidence rows. Measure routing candidate load, hot read models, write transaction, pool acquire, startup without migration, finalization under concurrent reads, and task/queue/memory bounds. Store raw machine/environment metadata and medians/p95 in the qualification audit.

The standard fixture, absolute release-profile measurements, bounded reader task fan-out, production write-queue snapshot, frozen V1 relative baseline, and complete controlled-machine raw metadata are captured. The retained paired record is `D:\Dev\Build\relay-pool-persistence-v2-qualification\paired-v0.3.1-v2.json`; its relative gates are green.

- [ ] **Step 3: Run all qualification gates**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_fault_matrix -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::performance_tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --lib persistence::differential_tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_upgrade_recovery -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::proxy::soak_tests -- --nocapture
pnpm.cmd test:contracts
pnpm.cmd build
```

Expected: hot read p95 is no more than 10% slower than baseline, routing p95 < 50 ms, ordinary write p95 < 100 ms, pool acquire p95 < 20 ms, and all queues/tasks remain bounded. Threshold changes require a new design review; do not edit constants to make red measurements green.

- [ ] **Step 4: Scan secrets and artifacts**

Scan every V2 text/blob column, logs, diagnostic JSON, upgrade reports, fixture files, Git index, and release bundle for seeded key/cookie/token/password/plaintext canaries. Assert no database, WAL, SHM, journal, backup, absolute user path, log, or generated target directory is tracked. The only tracked-database exception is an exact path in `scripts/persistence-v2-artifact-policy.json`; every exception must bind a tracked manifest SHA-256 and pass read-only SQLite integrity plus per-table/per-column text/blob scanning. Directory, glob, and extension-wide fixture exceptions are forbidden.

The unified scanner, fixture policy, negative Node contract, pre-release index gate, and post-Tauri final-bundle gate are implemented. `pnpm verify:persistence-artifacts` has passed for the tracked worktree. This step remains open until the final candidate bundle is generated and the post-build scan evidence is captured; a pre-build source scan alone is not completion evidence.

- [ ] **Step 5: Commit qualification evidence**

```powershell
git add -- src-tauri/tests/persistence_fault_matrix.rs src-tauri/src/persistence/performance_tests.rs src-tauri/src/persistence/differential_tests.rs docs/superpowers/audits/2026-07-18-persistence-v2-qualification.md scripts/release-verification-entrypoint.test.mjs scripts/run-contract-tests.mjs
git commit -m "test: qualify persistence v2 cutover"
```

### Task 16: Delete AppDatabase, rusqlite, and every temporary adapter

**Implementation status (2026-07-22):** `AppDatabase`, `services/database.rs`, production `rusqlite`, legacy proxy runtime, and temporary adapters have been deleted from the working tree. The Rust architecture gate passes (35 / 35) and the boundary manifest reflects the V2 adapters. `cargo fmt --check`, strict Clippy (`-D warnings`), Rust `--all-targets`, and `scripts/prepare-sqlx.ps1 -Check` have passed. This implementation moved ahead of the Task 15 prerequisite; final frontend/contract/build reruns, formal paired performance evidence, staged review, and commit remain pending.

**Files:**
- Delete: `src-tauri/src/services/database.rs`
- Delete: all temporary V1 adapters identified in `persistence-v2-boundary-manifest.json`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify/Delete: Node source-string tests that inspect `database.rs`
- Modify: `src-tauri/src/persistence/differential_tests.rs`
- Modify: `src-tauri/tests/persistence_upgrade/fixtures/profile_NNN/expected_manifest.json`
- Modify: `src-tauri/tests/persistence_architecture.rs`
- Modify: `docs/superpowers/audits/persistence-v2-boundary-manifest.json`

- [x] **Step 1: Make deletion gates fail before deletion**

```rust
#[test]
fn legacy_persistence_is_absent_from_release_source() {
    let graph = ParsedModuleGraph::load("src").unwrap();
    let manifest = BoundaryManifest::load(
        "../docs/superpowers/audits/persistence-v2-boundary-manifest.json",
    ).unwrap();
    assert!(!graph.has_symbol("AppDatabase"));
    assert!(!Path::new("src/services/database.rs").exists());
    assert!(!cargo_metadata().production_dependencies.contains("rusqlite"));
    assert_eq!(
        graph.production_consumers_of_prefix("persistence::legacy_import"),
        manifest.allowed_consumers("persistence::legacy_import"),
    );
}
```

Run and observe FAIL while the old file/dependency still exists.

- [ ] **Step 2: Delete only after Task 15 evidence is green**

Delete `database.rs`, legacy inline tests, old manual migration helpers, production `rusqlite`, compatibility facades, test-only production helpers, legacy Proxy persistence adapters, and every source-inspection Node test whose only contract is old implementation text. Freeze the already-green sanitized V1 outputs into each profile's `expected_manifest.json`, then remove executable V1 differential calls so final tests compare V2/import output against manifests without compiling `AppDatabase`. Keep released SQLite fixtures and read-only profile importer code.

The deletion itself is present and Task 15's paired performance prerequisite is now green. Keep this step reviewable until the exact-path staged snapshot and local commit are complete.

- [ ] **Step 3: Regenerate lockfile/offline metadata and run architecture gate**

```powershell
cargo check --manifest-path src-tauri/Cargo.toml
powershell -ExecutionPolicy Bypass -File scripts/prepare-sqlx.ps1
cargo test --manifest-path src-tauri/Cargo.toml --test persistence_architecture -- --nocapture
codegraph sync .
codegraph query AppDatabase
```

Expected: `AppDatabase` query has no production symbol; boundary manifest contains no temporary legacy exception; `rusqlite` is absent from production dependency tree.

The lockfile/Cargo build, fresh SQLx offline metadata check, and Rust architecture gate are green. The worktree still has no initialized CodeGraph index, and the final staged snapshot has not been committed, so the combined release step remains open.

- [ ] **Step 4: Run full local gate and commit deletion**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets -- --nocapture
pnpm.cmd test:contracts
pnpm.cmd build
cargo check --manifest-path src-tauri/Cargo.toml
$manifest = Get-Content -Raw docs/superpowers/audits/persistence-v2-boundary-manifest.json | ConvertFrom-Json
$released = Get-Content -Raw docs/superpowers/audits/persistence-v2-released-schema-manifest.json | ConvertFrom-Json
git add -- src-tauri/src/services/database.rs src-tauri/src/services/mod.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tests/persistence_architecture.rs src-tauri/src/persistence/differential_tests.rs docs/superpowers/audits/persistence-v2-boundary-manifest.json .sqlx
foreach ($path in $manifest.temporary_legacy_paths) { git add -- $path }
foreach ($profile in $released.profiles) { git add -- $profile.expected_manifest_path }
git diff --cached --name-only
git diff --cached --check
git commit -m "refactor: remove legacy persistence architecture"
```

Review the staged path list manually before commit. The manifest-driven loop stages only previously registered temporary legacy paths; unrelated mainline hunks must still be excluded with patch staging.

### Task 17: Verify signed Windows upgrade and release readiness

**Implementation status (2026-07-21):** Not complete. Local source/build gates do not substitute for a signed v0.3.1 updater/fresh-install/downgrade matrix on isolated Windows profiles.

**Files:**
- Create: `docs/superpowers/audits/2026-07-18-persistence-v2-release-evidence.md`

- [ ] **Step 1: Build the signed-package candidate**

```powershell
pnpm.cmd build
pnpm.cmd tauri build
```

Expected: frontend and Tauri release build exit 0; generated bundles contain no database, backup, log, secret fixture, or absolute local path.

- [ ] **Step 2: Run the signed upgrade matrix**

On isolated Windows user profiles, test fresh install plus upgrade from `v0.3.1`; default and custom data dirs; WAL source; pending relocation; disk full; killed process at every activation phase; old/new launch order; owner crash lock release; updater restart; incompatible schema inspection-only mode; downgrade instructions; real local Proxy streaming; Collector; Monitor; data recovery UI.

The harness must fully exit `v0.3.1` before launching V2. After tombstone, launching the supported generation-1 binary must hard fail without creating SQLite/WAL/SHM state. Earlier versions are outside the compatibility contract and must be rejected before mutation.

- [ ] **Step 3: Run the final staged snapshot gate**

```powershell
git add -- docs/superpowers/audits/2026-07-18-persistence-v2-release-evidence.md
git status --short
git diff --cached --name-only
git diff --cached --check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --all-targets -- --nocapture
pnpm.cmd test:contracts
pnpm.cmd build
cargo check --manifest-path src-tauri/Cargo.toml
node scripts/release-verification-entrypoint.test.mjs
codegraph status .
codegraph query AppDatabase
```

Expected: all commands exit 0; no staged unrelated path; CodeGraph has no `AppDatabase`, forbidden dependency edge, new high-fan-in owner, or unregistered public export.

- [ ] **Step 4: Commit release evidence**

```powershell
git add -- docs/superpowers/audits/2026-07-18-persistence-v2-release-evidence.md
git commit -m "docs: record persistence v2 release evidence"
```

Do not push or publish from this task without a separate explicit user request.
