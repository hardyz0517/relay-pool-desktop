import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const readiness = read("src/features/routing/routingMigrationReadiness.ts");
const panel = read("src/features/routing/RoutingMigrationReadinessPanel.tsx");
const preview = read("src/features/routing/RoutingOperationalPreviewPanel.tsx");
const page = read("src/features/routing/RoutingPage.tsx");
const generated = read("src/lib/bridge/generated.ts");
const registry = read("src-tauri/src/ipc/registry.rs");
const settingsStore = read("src-tauri/src/persistence/stores/settings_store.rs");
const commandFacadeMatrix = JSON.parse(read("docs/superpowers/audits/architecture-scale-command-facade-matrix.json"));

for (const [legacy, profile] of [
  ["priority_fallback", "priority_first"],
  ["stable_first", "priority_first"],
  ["cheap_first", "cost_first"],
  ["cost_stable_first", "cost_first"],
]) {
  assert.match(
    readiness,
    new RegExp(`case "${legacy}":[\\s\\S]*?return "${profile}"`, "u"),
    `${legacy} must have an explicit migration proposal`,
  );
}

for (const policy of ["backup_only", "automatic_balanced"]) {
  assert.match(
    readiness,
    new RegExp(`case "${policy}":[\\s\\S]*?return null`, "u"),
    `${policy} must require manual ordering-profile choice`,
  );
}

for (const issue of [
  "ordering_profile_unconfirmed",
  "multiplier_ceiling_unconfirmed",
  "group_scope_unconfirmed",
  "backup_depleted_unconfirmed",
  "affinity_unconfirmed",
]) {
  assert.match(readiness, new RegExp(issue, "u"), `readiness must track ${issue}`);
}

assert.match(
  readiness,
  /const input =\s*ready &&[\s\S]*?: null;/u,
  "readiness model must only emit a complete mutation input when ready and all nullable fields are narrowed",
);
assert.doesNotMatch(
  panel,
  /updateSettings|invokeCommand|@tauri-apps\/api/u,
  "migration UI must not save partial generic settings or invoke Tauri directly",
);
assert.match(
  page,
  /confirmHierarchicalRoutingMigration/u,
  "RoutingPage must use the explicit migration confirmation API",
);
assert.match(
  preview,
  /previewPolicyVersion/u,
  "preview UI must display the backend preview policy version",
);
assert.match(preview, /capacityMode/u, "preview UI must display capacity mode");
assert.match(
  preview,
  /selectedCapacityAcquired/u,
  "preview UI must show that preview simulation did not acquire capacity",
);
assert.match(
  generated,
  /confirmHierarchicalRoutingMigration/u,
  "generated bridge must expose migration confirmation",
);
assert.match(
  registry,
  /confirm_hierarchical_routing_migration/u,
  "IPC registry must register migration confirmation",
);
assert.match(
  settingsStore,
  /hierarchical_routing_migration_v1_json/u,
  "settings store must persist one complete hierarchical migration config",
);
assert.ok(
  commandFacadeMatrix.facades
    ?.find((facade) => facade.state_type === "SettingsStationsCommandFacade")
    ?.commands?.some(
      (entry) =>
        entry.command === "confirm_hierarchical_routing_migration" &&
        entry.use_case === "confirm_hierarchical_routing_migration" &&
        entry.dependencies?.includes("SettingsService"),
    ),
  "command facade matrix must register migration confirmation as a SettingsService use case",
);

console.log("routing migration readiness architecture checks passed");

function read(path) {
  return readFileSync(path, "utf8");
}
