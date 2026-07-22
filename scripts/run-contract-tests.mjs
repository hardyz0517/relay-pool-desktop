import { spawnSync } from "node:child_process";

const contracts = [
  ["node", ["scripts/manual-authorization-capability.test.mjs"]],
  ["node", ["scripts/tauri-command-fallback.test.mjs"]],
  ["node", ["scripts/tauri-error-classification-ownership.test.mjs"]],
  ["node", ["scripts/updater-config.test.mjs"]],
  ["node", ["scripts/updater-cleanup-contract.test.mjs"]],
  ["node", ["scripts/updater-current-version-fallback.test.mjs"]],
  ["node", ["scripts/updater-timeout-recovery.test.mjs"]],
  ["node", ["--test", "scripts/updater-check-coordinator.test.ts"]],
  ["node", ["scripts/dashboard-update-action.test.mjs"]],
  ["node", ["--test", "scripts/updater-error-message.test.ts"]],
  ["node", ["--test", "scripts/updater-state-flow.test.ts"]],
  ["node", ["scripts/updater-ui-contract.test.mjs"]],
  ["node", ["scripts/release-verification-entrypoint.test.mjs"]],
  ["node", ["scripts/release-version-contract.test.mjs"]],
  ["node", ["scripts/persistence-v2-artifact-scan.test.mjs"]],
  ["node", ["scripts/persistence-v2-fixture-manifest-contract.test.mjs"]],
  ["node", ["scripts/local-data-artifact-ignore.test.mjs"]],
  ["node", ["scripts/sqlx-offline-metadata.test.mjs"]],
  ["node", ["scripts/persistence-v1-performance-baseline.test.mjs"]],
  ["node", ["scripts/persistence-performance-qualification.test.mjs"]],
  ["node", ["scripts/data-store-diagnostic-redaction.test.mjs"]],
  ["node", ["scripts/data-store-upgrade-matrix.test.mjs"]],
  ["node", ["scripts/local-proxy-auth-contract.test.mjs"]],
  ["node", ["scripts/local-proxy-v2-boundary.test.mjs"]],
  ["node", ["scripts/request-lifecycle-architecture.test.mjs"]],
];

for (const [command, args] of contracts) {
  const result = spawnSync(command, args, { stdio: "inherit", shell: process.platform === "win32" });
  if (result.status !== 0) process.exit(result.status ?? 1);
}
