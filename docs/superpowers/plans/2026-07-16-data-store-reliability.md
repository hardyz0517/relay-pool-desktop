# Data Store Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent initialized installations from silently opening an empty/wrong SQLite database, provide an in-app path to select an existing healthy database, and preserve the supported settings-page data-directory move without unsafe overwrite or WAL loss.

**Architecture:** Add a narrow `services/data_store` domain around the existing `AppDatabase`: versioned config, read-only inspection, pure startup decision, consistent backup, and relocation. Tauri setup always publishes an immutable startup result; it registers `AppDatabase` and runners only for `Ready`, while `NeedsRecovery`/`Conflict` render a recovery-only frontend and complete selection through config commit plus cold restart. Existing schema/CRUD remain in `database.rs`; there is no hot database swap, row merge, generic recovery framework, or fake empty database.

**Tech Stack:** Rust 2021, Tauri 2, rusqlite 0.32 (`bundled`, `backup`), serde, Windows filesystem APIs for replace-existing config commit, React 18, TypeScript, TanStack Query, Vitest, Node contract scripts.

---

## Locked file map and size budget

Backend files to create:

```text
src-tauri/src/services/data_store/
  mod.rs        <= 260 lines; startup orchestration and immutable startup state
  types.rs      <= 240 lines; serialized types and internal candidate paths
  config.rs     <= 300 lines; v1/v2 read, marker, durable replace
  inspect.rs    <= 300 lines; READ_ONLY candidate inspection only
  decision.rs   <= 240 lines; pure decision table only
  backup.rs     <= 260 lines; consistent explicit-selection backup only
  relocation.rs <= 300 lines; settings-initiated move only
  diagnostic.rs <= 220 lines; sanitized export projection only
```

Existing backend boundaries:

- `src-tauri/src/services/database.rs`: keep `AppDatabase`, normal schema initialization/migrations, CRUD, and query logic. Remove config parsing/candidate copying only after equivalent focused tests are green.
- `src-tauri/src/lib.rs`: setup composition only; no candidate SQL or UI strings.
- `src-tauri/src/commands/mod.rs`: thin Tauri adapters only.
- `src-tauri/permissions/main-window.toml`: recovery command grants only; capture capability unchanged.

Frontend files to create:

```text
src/features/data-recovery/DataStoreBootstrap.tsx
src/features/data-recovery/DataRecoveryScreen.tsx
src/features/data-recovery/recoveryViewModel.ts
src/lib/api/dataRecovery.ts
src/lib/types/dataRecovery.ts
```

`DataStoreBootstrap` sits above `App`; do not add recovery branches to `AppShell`, individual pages, query services, or every business API. No new state library.

### Task 1: Freeze startup decision types and the pure decision table

**Files:**
- Create: `src-tauri/src/services/data_store/types.rs`
- Create: `src-tauri/src/services/data_store/decision.rs`
- Create: `src-tauri/src/services/data_store/mod.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: inline `#[cfg(test)]` table in `decision.rs`

- [ ] **Step 1: Write the table-driven tests first**

Define test inputs without filesystem access:

```rust
#[derive(Clone)]
struct CandidateFacts {
    id: String,
    health: CandidateHealth,
    contains_relay_pool_schema: bool,
    schema_compatible: bool,
}

struct DecisionInput {
    initialized: bool,
    active: Option<CandidateFacts>,
    candidates: Vec<CandidateFacts>,
    pending_relocation: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum StartupDecisionTag { Ready, FirstRun, NeedsRecovery, Conflict }

struct DecisionCase {
    name: &'static str,
    initialized: bool,
    active: Option<CandidateFacts>,
    candidates: Vec<CandidateFacts>,
    pending_relocation: bool,
    expected: StartupDecisionTag,
}

let cases = [
    DecisionCase::first_run(),
    DecisionCase::healthy_active(),
    DecisionCase::initialized_missing_active(),
    DecisionCase::one_legacy_candidate(),
    DecisionCase::two_populated_candidates(),
    DecisionCase::corrupt_active(),
    DecisionCase::pending_relocation(),
];
for case in cases {
    assert_eq!(decide_startup(&case.into_input()).tag(), case.expected, "{}", case.name);
}
```

Required expectations: only the uninitialized/no-database case returns `FirstRun`; a healthy configured active database returns `Ready`; an unmarked legacy install with a healthy default database also returns `Ready` and is committed only after open succeeds; missing/unreadable/corrupt returns `NeedsRecovery`; two healthy candidates containing protected Relay Pool schema return `Conflict`; pending relocation returns `NeedsRecovery(PendingRelocation)`.

`pending relocation` in this table means an ambiguous legacy v1 `pending/source` pair. A valid v2 config with `activeDataDir == sourceDataDir`, a distinct pending directory, and no fixed target database is a trusted user relocation intent handled by `relocation.rs` before this decision table; Task 9 defines that path.

- [ ] **Step 2: Run the focused test and verify RED**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::decision -- --nocapture
```

Expected: compile failure because the module and types do not exist.

- [ ] **Step 3: Implement the minimal closed type set**

Use these public shapes; do not add more top-level states:

```rust
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryReason {
    Missing, Unreadable, InvalidSqlite, IntegrityFailed,
    OpenOrMigrationFailed, PendingRelocation,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CandidateHealth { Healthy, Missing, Unreadable, InvalidSqlite, IntegrityFailed }

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CandidateRole { Active, Default, Source, Pending, Backup, Located }

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DataStoreCandidate {
    pub id: String,
    pub role: CandidateRole,
    pub path: String,
    pub health: CandidateHealth,
    pub schema_compatible: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub counts: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartupDecision {
    Ready { candidate_id: String },
    FirstRun { default_data_dir: PathBuf },
    NeedsRecovery { reason: RecoveryReason },
    Conflict { candidate_ids: Vec<String> },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataStoreStartupView {
    pub decision: StartupDecision,
    pub candidates: Vec<DataStoreCandidate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationResult {
    pub restart_required: bool,
}
```

`CandidateFacts` contains only `id`, role, health, `contains_relay_pool_schema`, and schema compatibility. Any existing database with a recognized Relay Pool table is protected state even when every allowlisted count is zero; counts are display/verification evidence, never permission to overwrite. `decide_startup(&DecisionInput) -> StartupDecision` must be deterministic and perform no I/O.

- [ ] **Step 4: Run focused tests and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::decision -- --nocapture
git add -- src-tauri/src/services/data_store/types.rs src-tauri/src/services/data_store/decision.rs src-tauri/src/services/data_store/mod.rs src-tauri/src/services/mod.rs
git commit -m "feat: define data store startup decisions"
```

Expected: focused tests pass; commit contains only the four paths.

### Task 2: Add versioned config and installation marker with durable commits

**Files:**
- Create: `src-tauri/src/services/data_store/config.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Test: inline config tests in `config.rs`

- [ ] **Step 1: Write config compatibility/failure tests**

Cover exact cases:

```rust
assert_eq!(read_config(v1_pending_source)?.version, 1);
assert_eq!(read_config(v2_active)?.active_data_dir, Some(active.clone()));
assert!(read_config(truncated_json).is_err());
assert_eq!(read_config_after_failed_commit, old_config);
assert!(!marker_exists_before_first_success);
assert!(marker_exists_after_first_success);
```

Also inject failure immediately before replace and assert the old config bytes remain unchanged and the temporary file is ignored on the next read.

- [ ] **Step 2: Verify RED**

Run `cargo test --manifest-path src-tauri/Cargo.toml services::data_store::config -- --nocapture`.

Expected: compile failure for missing config functions.

- [ ] **Step 3: Implement only the v2 fields from the spec**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DataDirConfigV2 {
    pub version: u32,
    pub active_data_dir: Option<PathBuf>,
    pub pending_data_dir: Option<PathBuf>,
    pub source_data_dir: Option<PathBuf>,
    pub updated_at: String,
}
```

Keep `installation.marker` as a separate zero-content file in default AppData. Do not add installation/database UUIDs. Read legacy `pendingDataDir/sourceDataDir` without rewriting it.

- [ ] **Step 4: Implement durable replace without delete-then-rename**

Write JSON to a same-directory temporary file, call `File::sync_all`, then:

- on Windows, add direct `windows-sys` target dependency with `Win32_Storage_FileSystem` and use `ReplaceFileW` when destination exists; use same-directory `std::fs::rename` only for the first create;
- sync the parent directory where supported;
- preserve the old config and return an error if replacement fails.

Do not call `remove_file(destination)` before replacement.

- [ ] **Step 5: Run tests/check and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::config -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/data_store/config.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: persist data store selection safely"
```

### Task 3: Inspect candidates without creating or mutating SQLite files

**Files:**
- Create: `src-tauri/src/services/data_store/inspect.rs`
- Modify: `src-tauri/src/services/data_store/types.rs`
- Test: inline inspection tests in `inspect.rs`

- [ ] **Step 1: Write filesystem and SQLite inspection tests**

Tests must prove:

- missing candidate returns `Missing` and neither file nor parent directory is created;
- invalid header returns `InvalidSqlite`;
- healthy schema returns non-sensitive counts and `contains_relay_pool_schema = true`;
- a database containing only one recognized Relay Pool table is still protected state even when all row counts are zero;
- `PRAGMA quick_check` failure returns `IntegrityFailed`;
- read-only file inspection performs no schema initialization and does not change file size/mtime;
- station names, URLs, credentials, cookies, and secrets never appear in the serialized summary.

- [ ] **Step 2: Verify RED**

Run `cargo test --manifest-path src-tauri/Cargo.toml services::data_store::inspect -- --nocapture`.

- [ ] **Step 3: Implement strict read-only open**

Use:

```rust
Connection::open_with_flags(
    path,
    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
)
```

Check `path.is_file()` before SQLite open. Query only header/quick-check/schema existence and an allowlisted count set (`stations`, `station_keys`, `channel_monitors`, `settings`). Set `contains_relay_pool_schema` when any recognized application table exists; do not use zero counts to mark the file disposable. Serialize a generated candidate ID, local display path, role, health, metadata, and counts. Diagnostic export removes the display path later.

- [ ] **Step 4: Run tests and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::inspect -- --nocapture
git add -- src-tauri/src/services/data_store/inspect.rs src-tauri/src/services/data_store/types.rs
git commit -m "feat: inspect data store candidates read only"
```

### Task 4: Create consistent explicit-selection backups

**Files:**
- Create: `src-tauri/src/services/data_store/backup.rs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Test: inline backup tests in `backup.rs`

- [ ] **Step 1: Write WAL and failure tests**

Create a source in WAL mode, insert committed rows, leave WAL present, then assert the backup contains the same allowlisted counts and passes `quick_check`. Add injected failures for insufficient-space preflight and destination-open failure; source bytes/counts must remain unchanged.

- [ ] **Step 2: Verify RED**

Run `cargo test --manifest-path src-tauri/Cargo.toml services::data_store::backup -- --nocapture`.

- [ ] **Step 3: Enable and use rusqlite's backup API**

Change dependency to:

```toml
rusqlite = { version = "0.32", features = ["bundled", "backup"] }
```

Implement with the verified 0.32 API:

```rust
let source = Connection::open_with_flags(source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
let mut destination = Connection::open(&temp_backup_path)?;
let backup = rusqlite::backup::Backup::new(&source, &mut destination)?;
backup.run_to_completion(5, Duration::from_millis(250), None)?;
```

Write under `default_app_data/backups/<UTC timestamp>/relay-pool-desktop.sqlite3`, verify before exposing it, and never overwrite an existing backup directory. The fixed database filename lets the existing activation validation select a backup explicitly; the separate owned `backups/` root prevents normal default/source/pending discovery from mistaking it for the active database. Scan only one level under the owned `backups/` directory.

- [ ] **Step 4: Run tests/check and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::backup -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/data_store/backup.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: back up selected databases consistently"
```

### Task 5: Compose candidate discovery and immutable startup state

**Files:**
- Modify: `src-tauri/src/services/data_store/mod.rs`
- Modify: `src-tauri/src/services/data_store/types.rs`
- Test: inline orchestration tests in `mod.rs`

- [ ] **Step 1: Write orchestration tests**

Use temporary default/source/pending directories and assert:

- first run is the only path allowed to create the default directory/database later; discovery itself creates nothing;
- marker + missing active becomes `NeedsRecovery(Missing)`;
- one healthy legacy source is suggested but not auto-committed;
- two populated healthy candidates become `Conflict`;
- legacy v1 `pending != source` becomes `PendingRelocation`;
- v2 active/source/pending intent is returned as an internal relocation action only when active equals source and target fixed DB does not exist;
- owned backups are candidates; arbitrary filesystem locations are not scanned.

- [ ] **Step 2: Verify RED, then implement the narrow API**

```rust
pub struct DataStoreStartupState {
    pub decision: StartupDecision,
    pub candidates: Vec<DataStoreCandidate>,
    default_data_dir: PathBuf,
}

pub fn inspect_startup(default_data_dir: &Path) -> Result<DataStoreStartupState, String>;
```

`DataStoreCandidate` includes a local display/activation path; its `id` is only a stable UI key for this inspection result. `DataStoreStartupState::view()` returns `DataStoreStartupView` without exposing private `default_data_dir`. Activation accepts the path and canonicalizes plus re-inspects it on the backend, so a manually located file does not require mutating startup state. The state is constructed once per cold start. Do not add setters, a mutable candidate registry, or runtime database swapping.

- [ ] **Step 3: Run focused tests and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::data_store -- --nocapture
git add -- src-tauri/src/services/data_store/mod.rs src-tauri/src/services/data_store/types.rs
git commit -m "feat: decide data store startup safely"
```

### Task 6: Separate normal database initialization from selection/config mutation

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Test: Rust tests in `database.rs`

- [ ] **Step 1: Add regression tests around initialization order**

Add focused tests proving the two explicit entry points:

- `initialize_existing_at` opens the exact existing path provided by startup orchestration and errors when the fixed database file is absent;
- `initialize_new_at` errors when the fixed database file already exists and is the only path allowed to create a database;
- does not read or rewrite data-dir config;
- reports open/schema migration failure without marking source initialized;
- preserves existing station/settings tests.

- [ ] **Step 2: Verify RED**

Run `cargo test --manifest-path src-tauri/Cargo.toml initialize_at -- --nocapture`.

- [ ] **Step 3: Extract only the normal open path**

Introduce two methods sharing a private `initialize_connection` after their opposite precondition checks:

```rust
pub fn initialize_existing_at(
    default_data_dir: PathBuf,
    active_data_dir: PathBuf,
    pending_data_dir: Option<PathBuf>,
) -> Result<Self, String>;

pub fn initialize_new_at(
    default_data_dir: PathBuf,
    active_data_dir: PathBuf,
) -> Result<Self, String>;
```

The existing variant checks `is_file()` before SQLite open; the new variant checks `!exists()` and creates the parent directory before SQLite open. Both then run the existing schema/migration chain unchanged and build `AppDatabase`. Neither reads or writes data-dir config. Keep old config/copy helpers temporarily until Task 9 replaces settings relocation and their tests.

- [ ] **Step 4: Run broad database tests and commit**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::database -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/database.rs
git commit -m "refactor: separate database open from data store selection"
```

### Task 7: Make Tauri setup fail closed but keep recovery UI available

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/permissions/main-window.toml`
- Modify: `src-tauri/src/services/data_store/mod.rs`
- Create: `src-tauri/src/services/secrets/validation.rs`
- Modify: `src-tauri/src/services/secrets/mod.rs`
- Test: create `scripts/data-store-startup-boundary.test.mjs` plus Rust setup-helper tests

- [ ] **Step 1: Write the boundary contract RED test**

Assert source contains all of these boundaries:

```js
assert.match(lib, /inspect_startup/);
assert.match(lib, /DataStoreStartupState/);
assert.doesNotMatch(lib, /AppDatabase::initialize\(app\.handle\(\)\)\?/);
assert.match(commands, /get_data_store_startup_state/);
assert.match(commands, /activate_data_store_candidate/);
assert.match(permission, /get_data_store_startup_state/);
assert.match(permission, /activate_data_store_candidate/);
```

- [ ] **Step 2: Add thin recovery commands**

```rust
#[tauri::command]
pub fn get_data_store_startup_state(
    state: State<'_, DataStoreStartupState>,
) -> DataStoreStartupView { state.view() }

#[tauri::command]
pub fn activate_data_store_candidate(
    state: State<'_, DataStoreStartupState>,
    secrets: State<'_, SecretManager>,
    candidate_path: String,
) -> Result<ActivationResult, String>
```

Activation canonicalizes the supplied path, requires the fixed database filename, re-inspects it read-only, verifies existing encrypted secret rows with `secrets.data_key()` without logging values, creates/verifies backup, commits v2 active config, and returns `{ restartRequired: true }`. It must not trust health/count fields sent by the frontend. The frontend then calls `relaunch()` from `@tauri-apps/plugin-process`; if relaunch rejects, it displays “配置已保存，请手动重启”. The backend never registers a database in the current process. Task 10 separately defines `open_data_store_backup_dir` and `export_data_store_diagnostic`.

Implement `secrets::validation::validate_database_secrets(path, data_key)` by opening the existing DB read-only, returning success when the `secrets` table is absent/empty, mapping each row into the existing `EncryptedPayload`, and calling `crypto::decrypt_secret`. It returns only a sanitized row identifier/error code; it never returns or logs plaintext, ciphertext, nonce, AAD, or hashes. Tests cover empty table, successful decryption, wrong key, and redacted error text.

- [ ] **Step 3: Compose setup without fake database state**

Move setup logic into a small helper returning either `Ready(AppDatabase)` or recovery state. Always manage `DataStoreStartupState`. `Ready` must call `initialize_existing_at`; `FirstRun` alone calls `initialize_new_at`. Manage `AppDatabase`, monitor runner, and collector runner only after successful initialization. For `FirstRun`, commit v2 `activeDataDir` and create the installation marker only after database open plus schema initialization succeeds; failure must leave neither marker nor committed active config. For an unmarked legacy install with a healthy default DB, open first, then commit v2 config plus marker. If open/migration fails, convert it to `NeedsRecovery(OpenOrMigrationFailed)` and return `Ok(())` from Tauri setup. Remove the ambiguous public `AppDatabase::initialize(app)` after setup no longer calls it.

- [ ] **Step 4: Verify boundary and backend**

```powershell
node scripts/data-store-startup-boundary.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml services::data_store -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 5: Commit**

```powershell
git add -- src-tauri/src/lib.rs src-tauri/src/commands/mod.rs src-tauri/permissions/main-window.toml src-tauri/src/services/data_store/mod.rs src-tauri/src/services/secrets/validation.rs src-tauri/src/services/secrets/mod.rs scripts/data-store-startup-boundary.test.mjs
git commit -m "feat: keep recovery available when database startup fails"
```

### Task 8: Gate the React application above every business query

**Files:**
- Create: `src/lib/types/dataRecovery.ts`
- Create: `src/lib/api/dataRecovery.ts`
- Create: `src/features/data-recovery/recoveryViewModel.ts`
- Create: `src/features/data-recovery/DataStoreBootstrap.tsx`
- Create: `src/features/data-recovery/DataRecoveryScreen.tsx`
- Modify: `src/main.tsx`
- Test: create `src/features/data-recovery/recoveryViewModel.test.ts`
- Test: create `src/features/data-recovery/DataStoreBootstrap.test.tsx`

- [ ] **Step 1: Define matching TypeScript contracts and tests**

```ts
export type DataStoreStartupDecision =
  | { kind: "ready"; candidateId: string }
  | { kind: "firstRun"; defaultDataDir: string }
  | { kind: "needsRecovery"; reason: RecoveryReason }
  | { kind: "conflict"; candidateIds: string[] };

export type RecoveryReason =
  | "missing" | "unreadable" | "invalidSqlite"
  | "integrityFailed" | "openOrMigrationFailed" | "pendingRelocation";

export type DataStoreCandidate = {
  id: string;
  role: "active" | "default" | "source" | "pending" | "backup" | "located";
  path: string;
  health: "healthy" | "missing" | "unreadable" | "invalidSqlite" | "integrityFailed";
  schemaCompatible: boolean;
  sizeBytes: number | null;
  modifiedAt: string | null;
  counts: Record<string, number>;
};

export type DataStoreStartupView = {
  decision: DataStoreStartupDecision;
  candidates: DataStoreCandidate[];
};
```

Test that ACL errors from `get_data_store_startup_state` render a fatal error and do not fall back; only genuine non-Tauri browser preview returns a documented preview-ready state. Test that `App` is never rendered before `ready`.

- [ ] **Step 2: Verify RED**

Run `pnpm exec vitest run src/features/data-recovery`. Expected: missing modules/tests fail.

- [ ] **Step 3: Implement the outer bootstrap**

`DataStoreBootstrap` owns only startup loading/error/recovery state:

```tsx
if (loading) return <StartupLoadingScreen />;
if (error) return <StartupFatalError error={error} onRetry={reload} />;
if (state.decision.kind !== "ready") {
  return <DataRecoveryScreen state={state} onActivated={reload} />;
}
return <App />;
```

In `src/main.tsx`, replace `<App />` with `<DataStoreBootstrap />`. Do not modify `AppShell` or business query files.

- [ ] **Step 4: Implement a minimal truthful recovery screen**

Show local path, role, size, mtime, health, schema status, and allowlisted counts. Selection requires confirmation; corrupt/unreadable candidates are disabled. In this task, actions are choose an already discovered healthy candidate and exit. Render locate database, create new database, export diagnostics, and open backup folder only after Task 10 adds those backend commands. Do not offer row merge or “recommended automatically” wording when multiple populated candidates exist.

- [ ] **Step 5: Verify frontend and commit**

```powershell
pnpm exec vitest run src/features/data-recovery
pnpm build
git add -- src/lib/types/dataRecovery.ts src/lib/api/dataRecovery.ts src/features/data-recovery/recoveryViewModel.ts src/features/data-recovery/recoveryViewModel.test.ts src/features/data-recovery/DataStoreBootstrap.tsx src/features/data-recovery/DataStoreBootstrap.test.tsx src/features/data-recovery/DataRecoveryScreen.tsx src/main.tsx
git commit -m "feat: show recovery before business pages mount"
```

### Task 9: Replace settings-page relocation without reusing recovery selection

**Files:**
- Create: `src-tauri/src/services/data_store/relocation.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `scripts/settings-data-dir.test.mjs`
- Test: inline relocation tests

- [ ] **Step 1: Write the relocation matrix first**

Required Rust cases: source WAL copied consistently to nonexistent target; populated target rejected; any target with a recognized Relay Pool table rejects overwrite even when row counts are zero; backup/validation failure keeps source active; config commit failure keeps source active; any old v1 target becomes `PendingRelocation` rather than auto-overwrite.

- [ ] **Step 2: Verify RED**

Run `cargo test --manifest-path src-tauri/Cargo.toml services::data_store::relocation -- --nocapture`.

- [ ] **Step 3: Implement an explicit relocation intent**

`choose_data_dir` writes v2 config with unchanged `activeDataDir`, `sourceDataDir == activeDataDir`, and the selected `pendingDataDir`; this is the only code allowed to create a trusted relocation intent. On cold start, a trusted v2 intent creates a consistent SQLite backup into a target temporary filename, validates it, atomically renames only when the fixed target DB does not exist, then commits `activeDataDir`. Legacy v1 pending/source remains `PendingRelocation` and requires user confirmation. Relocation never calls recovery activation and never overwrites an existing fixed target.

Delete `should_copy_source_database`, `inspect_sqlite_user_state`, and the raw `fs::copy` path only after the new tests pass. Update `settings-data-dir.test.mjs` to assert the relocation module and forbid `fs::copy(&source_db_path, &db_path)`.

- [ ] **Step 4: Verify all persistence regressions**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::relocation -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml pending_data_dir_activation -- --nocapture
node scripts/settings-data-dir.test.mjs
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all pass; if old test names are replaced, keep equivalent assertions for stations, settings-only source, and non-overwrite target.

- [ ] **Step 5: Commit**

```powershell
git add -- src-tauri/src/services/data_store/relocation.rs src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs scripts/settings-data-dir.test.mjs
git commit -m "fix: relocate data directories without overwriting state"
```

### Task 10: Add sanitized diagnostics, manual locate/new database, and release gates

**Files:**
- Create: `src-tauri/src/services/data_store/diagnostic.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/permissions/main-window.toml`
- Modify: `src/lib/api/dataRecovery.ts`
- Modify: `src/features/data-recovery/DataRecoveryScreen.tsx`
- Create: `scripts/data-store-diagnostic-redaction.test.mjs`
- Create: `scripts/data-store-upgrade-matrix.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`
- Create: `docs/release/DATA_STORE_RELIABILITY_SMOKE_CHECKLIST.md`

- [ ] **Step 1: Write diagnostic projection tests**

Build a candidate containing a username path, station name, URL, API key, cookie, ciphertext metadata, and request body. Assert exported JSON contains only app/OS/config versions, marker boolean, candidate role plus per-report anonymous ID, size/mtime, schema version, allowlisted counts, check results, phase, and sanitized error code. Assert every injected sensitive string is absent.

- [ ] **Step 2: Implement thin commands**

Add:

```rust
get_data_store_startup_state
refresh_data_store_candidates
locate_data_store_candidate
activate_data_store_candidate
create_new_data_store
open_data_store_backup_dir
export_data_store_diagnostic
```

`locate` uses a native file dialog and rejects any selected filename other than `relay-pool-desktop.sqlite3`, then read-only inspects it. `create_new_data_store` requires `confirmed: true`, accepts a directory selected by the native folder dialog, rejects it if the fixed database path already exists, calls `AppDatabase::initialize_new_at`, drops that unregistered database handle, and commits `activeDataDir` plus installation marker only after schema initialization succeeds. It returns `{ restartRequired: true }`; the frontend calls `relaunch()`. Register and grant exactly these commands to main-window; capture permissions remain unchanged.

- [ ] **Step 3: Add upgrade-matrix fixture runner**

The Node contract verifies fixture coverage names for: clean install, healthy default, healthy custom active, missing/unreadable active, invalid header, integrity failure, legacy v1 source/pending, empty pending with populated source, source+target conflict, default+source+pending conflict, marker+missing DB, truncated config, WAL relocation, and config-commit failure. Each fixture invokes a focused Rust test by name and fails fast. Add both new Node scripts to `run-contract-tests.mjs`.

- [ ] **Step 4: Run full verification**

```powershell
pnpm test:contracts
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml services::data_store -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::database -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all exit 0. No timeout, ignored fixture, or “no tests found” counts as success.

- [ ] **Step 5: Build and run the real updater/cold-start checklist**

Checklist must include two disposable VMs/profiles: previous release with default DB and previous release with custom/pending config. Record pre-update active path plus station/key/settings/monitor/log counts; update through the real updater; open pricing/channel/change-center/stations; exit from tray; cold start; verify path/counts match. Inject missing DB and source/target conflict to verify recovery screen, no business queries, no runners, manual selection, backup creation, restart, and intact unselected DB.

Run:

```powershell
pnpm verify:release
pnpm tauri:build -- --target x86_64-pc-windows-msvc
```

- [ ] **Step 6: Commit the final data reliability gate**

```powershell
git add -- src-tauri/src/services/data_store/diagnostic.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/permissions/main-window.toml src/lib/api/dataRecovery.ts src/features/data-recovery/DataRecoveryScreen.tsx scripts/data-store-diagnostic-redaction.test.mjs scripts/data-store-upgrade-matrix.test.mjs scripts/run-contract-tests.mjs docs/release/DATA_STORE_RELIABILITY_SMOKE_CHECKLIST.md
git diff --cached --name-only
git commit -m "test: gate data store reliability releases"
```

## Plan completion gate

Do not publish internal Stage 2 without Stage 3. The data reliability release is eligible only when all commands below pass on the same revision and the real updater checklist is recorded outside Git without secrets:

```powershell
pnpm verify:release
cargo test --manifest-path src-tauri/Cargo.toml services::data_store -- --nocapture
pnpm tauri:build -- --target x86_64-pc-windows-msvc
git diff --exit-code -- src-tauri/src/services/data_store src-tauri/src/services/database.rs src-tauri/src/services/mod.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/permissions/main-window.toml src/lib/types/dataRecovery.ts src/lib/api/dataRecovery.ts src/features/data-recovery src/main.tsx scripts/data-store-startup-boundary.test.mjs scripts/data-store-diagnostic-redaction.test.mjs scripts/data-store-upgrade-matrix.test.mjs scripts/settings-data-dir.test.mjs scripts/run-contract-tests.mjs docs/release/DATA_STORE_RELIABILITY_SMOKE_CHECKLIST.md
git status --short
```

Pre-existing user changes may remain visible in `git status`, but task commits must use exact paths. Never stage local databases, backups, diagnostics, `output/`, target directories, logs, or `src-tauri/src/services/collectors/adapters/newapi/test_support.rs` unless separately authorized.
