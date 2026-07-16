import { invoke } from "@tauri-apps/api/core";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type { SecretMigrationReport, SecretScanFinding } from "@/lib/types/secrets";

export function getSecretMigrationStatus() {
  return invoke<SecretMigrationReport>("get_secret_migration_status").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return {
        migratedCount: 0,
        skippedCount: 0,
        failedCount: 0,
        failures: [],
      };
    }
    throw error;
  });
}

export function runSecretSafetyScan() {
  return invoke<SecretScanFinding[]>("run_secret_safety_scan").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return [];
    }
    throw error;
  });
}
