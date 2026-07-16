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
];

for (const [command, args] of contracts) {
  const result = spawnSync(command, args, { stdio: "inherit", shell: process.platform === "win32" });
  if (result.status !== 0) process.exit(result.status ?? 1);
}
