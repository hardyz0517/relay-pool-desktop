import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";

const migrationNames = await readdir("src-tauri/src/persistence/migrations");
assert.ok(
  migrationNames.includes("0017_encrypted_secret_baseline.sql"),
  "0017 encrypted-secret baseline migration must exist after the mainline schema 16 monitor defaults migration",
);

const migration = await readFile(
  "src-tauri/src/persistence/migrations/0017_encrypted_secret_baseline.sql",
  "utf8",
);
const baseline = await readFile("src-tauri/src/services/secrets/baseline_conversion.rs", "utf8");
const generationUpgrade = await readFile(
  "src-tauri/src/services/data_store/generation_upgrade.rs",
  "utf8",
);
const startupUpgradeExecutor = await readFile(
  "src-tauri/src/services/data_store/startup_upgrade_executor.rs",
  "utf8",
);
const runtime = await readFile("src-tauri/src/persistence/runtime.rs", "utf8");
const migrations = await readFile("src-tauri/src/persistence/migrations.rs", "utf8");
const schemaRegistry = await readFile("src-tauri/src/persistence/schema_registry.rs", "utf8");
const upgradeJournal = await readFile("src-tauri/src/persistence/upgrade_journal.rs", "utf8");
const recoveryExecutor = await readFile(
  "src-tauri/src/persistence/upgrade_recovery_executor.rs",
  "utf8",
);
const backup = await readFile("src-tauri/src/services/data_store/backup.rs", "utf8");

assert.ok(
  migration.includes("ALTER TABLE secrets ADD COLUMN key_id TEXT") &&
    migration.includes("ALTER TABLE secrets ADD COLUMN encryption_version INTEGER") &&
    migration.includes("CREATE TABLE app_secret_bindings") &&
    !migration.includes("SET schema_version = 17"),
  "0017 should add transitional structure without directly activating the encrypted-secret profile",
);

assert.ok(
  baseline.includes("ensure_active_database_baseline") &&
    baseline.includes("create_security_baseline_backup") &&
    baseline.includes("copy_sqlite_database") &&
    baseline.includes("publish_prepared_database") &&
    baseline.includes("detect_plaintext_secret_conflicts") &&
    baseline.includes("rebuild_secrets_with_final_constraints") &&
    baseline.includes("canonical_secret_aad") &&
    baseline.includes("UPDATE settings SET value = ''"),
  "baseline converter should use backup/staging/atomic publish, fail closed on plaintext conflicts, rekey with canonical AAD, and clear legacy settings.local_key",
);

assert.ok(
  upgradeJournal.includes("BaselineConversionPhase") &&
    upgradeJournal.includes("encryptedSecretBaseline") &&
    upgradeJournal.includes("CandidateBuilt") &&
    recoveryExecutor.includes("observe_persistence_journal") &&
    recoveryExecutor.includes("PersistenceJournalKind::BaselineConversion") &&
    recoveryExecutor.includes("write_baseline_conversion_journal_atomically") &&
    baseline.includes("run_journaled_conversion") &&
    baseline.includes("execute_baseline_candidate_validated"),
  "encrypted-secret baseline conversion must use a distinguishable journal kind and resumable phase machine",
);

assert.ok(
  baseline.includes("baseline_migration_metadata") && !baseline.includes("X''"),
  "baseline finalizer must record the real embedded sqlx migration checksum, not an empty checksum",
);

assert.ok(
  backup.includes("write_security_baseline_backup_metadata") &&
    backup.includes("old-security-format-local-machine-only"),
  "baseline conversion backups must carry non-sensitive metadata marking the old local-only security format",
);

assert.ok(
  generationUpgrade.includes("prepare_generation_two_with_resolver") &&
    generationUpgrade.includes("initialize_pre_baseline_runtime_for_import") &&
    generationUpgrade.includes("finalize_pre_baseline_database") &&
    generationUpgrade.includes("execute_startup_upgrade_plan") &&
    generationUpgrade.includes("observe_persistence_journal") &&
    generationUpgrade.includes("PersistenceJournalKind::BaselineConversion") &&
    startupUpgradeExecutor.includes("StartupUpgradeStep::EnsureSecretBaseline") &&
    startupUpgradeExecutor.includes("ensure_active_database_baseline"),
  "startup generation preparation should run baseline conversion before opening a writable generation-2 runtime",
);

assert.ok(
  !runtime.includes("crate::services::secrets") &&
    migrations.includes("schema_registry::current_binary_compatibility()") &&
    schemaRegistry.includes("readable_schema: 1..=latest") &&
    schemaRegistry.includes("writable_schema: BTreeSet::from([latest])"),
  "persistence runtime must stay service-independent while binary compatibility is derived from registry latest schema",
);
