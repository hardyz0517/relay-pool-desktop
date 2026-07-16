import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";

const cargoManifest = "src-tauri/Cargo.toml";
const relocationSource = await readFile("src-tauri/src/services/data_store/relocation.rs", "utf8");
const startupSource = await readFile("src-tauri/src/services/data_store/mod.rs", "utf8");
const inspectSource = await readFile("src-tauri/src/services/data_store/inspect.rs", "utf8");
const configSource = await readFile("src-tauri/src/services/data_store/config.rs", "utf8");
const bootstrapSource = await readFile("src/features/data-recovery/DataStoreBootstrap.tsx", "utf8");

const matrix = [
  {
    risk: "clean install",
    source: startupSource,
    evidence: ["first-run", "StartupDecision::FirstRun"],
    cargoFilter: "startup_orchestration_discovers_without_mutation_or_trust_leaks",
  },
  {
    risk: "healthy default database",
    source: startupSource,
    evidence: ["legacy-default", "StartupDecision::Ready"],
    cargoFilter: "startup_orchestration_discovers_without_mutation_or_trust_leaks",
  },
  {
    risk: "healthy custom active database",
    source: startupSource,
    evidence: ["healthy-custom-active", "CandidateRole::Active"],
    cargoFilter: "startup_orchestration_discovers_without_mutation_or_trust_leaks",
  },
  {
    risk: "missing active database",
    source: startupSource,
    evidence: ["marker-missing", "RecoveryReason::Missing"],
    cargoFilter: "startup_orchestration_discovers_without_mutation_or_trust_leaks",
  },
  {
    risk: "invalid header and integrity failure",
    source: inspectSource,
    evidence: ["invalid-header", "quick-check", "CandidateHealth::IntegrityFailed"],
    cargoFilter: "missing_invalid_and_integrity_failed_are_classified_without_creating_paths",
  },
  {
    risk: "legacy v1 source pending",
    source: startupSource,
    evidence: ["legacy-pending", "RecoveryReason::PendingRelocation"],
    cargoFilter: "startup_orchestration_discovers_without_mutation_or_trust_leaks",
  },
  {
    risk: "empty pending with populated source",
    source: startupSource,
    evidence: ["empty-pending-populated-source", "DataStoreRelocationIntent"],
    cargoFilter: "startup_orchestration_discovers_without_mutation_or_trust_leaks",
  },
  {
    risk: "source and target conflict",
    source: relocationSource,
    evidence: ["target-conflict", "target database already exists"],
    cargoFilter: "relocation_rejects_populated_or_protected_targets_without_changing_source_or_config",
  },
  {
    risk: "default source pending conflict",
    source: startupSource,
    evidence: ["default-source-pending-conflict", "StartupDecision::Conflict"],
    cargoFilter: "startup_orchestration_discovers_without_mutation_or_trust_leaks",
  },
  {
    risk: "truncated config",
    source: `${configSource}\n${startupSource}`,
    evidence: ["truncated-config", "startup_orchestration_recovers_from_truncated_config_without_opening_database"],
    cargoFilter: "startup_orchestration_recovers_from_truncated_config_without_opening_database",
  },
  {
    risk: "WAL relocation",
    source: relocationSource,
    evidence: ["wal-relocation", "enable_wal_and_insert"],
    cargoFilter: "relocation_copies_wal_source_consistently_to_missing_target_and_commits_active",
  },
  {
    risk: "config commit failure",
    source: configSource,
    evidence: ["failed-replace", "failed_replace_preserves_previous_config_and_ignores_temp_file"],
    cargoFilter: "failed_replace_preserves_previous_config_and_ignores_temp_file",
  },
  {
    risk: "backup or validation failure keeps source active",
    source: relocationSource,
    evidence: ["bad-source", "relocation_backup_or_validation_failure_keeps_source_active"],
    cargoFilter: "relocation_backup_or_validation_failure_keeps_source_active",
  },
  {
    risk: "legacy relocation intent is not trusted",
    source: relocationSource,
    evidence: ["legacy-intent", "non_trusted_legacy_config_is_not_relocated"],
    cargoFilter: "non_trusted_legacy_config_is_not_relocated",
  },
];

const executedFilters = new Set();
for (const item of matrix) {
  for (const token of item.evidence) {
    assert.ok(item.source.includes(token), `upgrade matrix should keep ${item.risk} evidence: ${token}`);
  }
  if (executedFilters.has(item.cargoFilter)) continue;
  const result = spawnSync(
    "cargo",
    ["test", "--manifest-path", cargoManifest, item.cargoFilter, "--", "--nocapture"],
    { stdio: "inherit", shell: process.platform === "win32" },
  );
  assert.equal(result.status, 0, `rust fixture test failed for ${item.risk}: ${item.cargoFilter}`);
  executedFilters.add(item.cargoFilter);
}

assert.ok(
  bootstrapSource.includes("state.decision.kind !== \"ready\"") &&
    bootstrapSource.includes("DataRecoveryScreen") &&
    !bootstrapSource.includes("AppShell"),
  "frontend upgrade recovery gate should stay above business pages",
);
