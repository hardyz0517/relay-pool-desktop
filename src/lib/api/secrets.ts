import { invoke } from "@tauri-apps/api/core";
import type { SecretMigrationReport, SecretScanFinding } from "@/lib/types/secrets";

export function getSecretMigrationStatus() {
  return invoke<SecretMigrationReport>("get_secret_migration_status");
}

export function runSecretSafetyScan() {
  return invoke<SecretScanFinding[]>("run_secret_safety_scan");
}
