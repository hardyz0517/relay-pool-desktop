import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const esbuild = require("../node_modules/.pnpm/node_modules/esbuild");

const outFile = resolve(tmpdir(), "relay-pool-channel-monitor-view-model.test.mjs");
await mkdir(dirname(outFile), { recursive: true });
await esbuild.build({
  entryPoints: ["src/lib/channelMonitorViewModel.ts"],
  outfile: outFile,
  bundle: true,
  platform: "node",
  format: "esm",
  external: ["react", "lucide-react", "@tauri-apps/api/core"],
});

const {
  createStationKeyMonitorInput,
  findStationKeyMonitor,
  preferredStationKeyMonitorTemplate,
  selectStationKeyMonitorModel,
  updateStationKeyMonitorEnabledInput,
} = await import(pathToFileURL(outFile).href);

const key = {
  id: "key-1",
  stationId: "station-1",
  name: "Primary Key",
};
const template = {
  id: "builtin-openai-chat-low-token",
  enabled: true,
};
const capabilities = {
  modelAllowlist: ["gpt-4.1", "gpt-4.1-mini", "claude-sonnet-4"],
  modelBlocklist: [],
  preferredModels: ["gpt-4.1"],
};

assert.equal(
  preferredStationKeyMonitorTemplate([
    { id: "builtin-openai-chat-low-token", enabled: true, endpointKind: "chat_completions" },
    { id: "builtin-openai-responses-low-token", enabled: true, endpointKind: "responses" },
  ])?.id,
  "builtin-openai-responses-low-token",
  "key-pool monitor switch should prefer the built-in Responses low-token template by default",
);

assert.equal(
  selectStationKeyMonitorModel(capabilities),
  "gpt-4.1-mini",
  "key-pool monitor switch should choose the lowest explicit model this key can call",
);

assert.equal(
  selectStationKeyMonitorModel({
    modelAllowlist: ["gpt-4o-mini", "gpt-4.1-mini"],
    modelBlocklist: ["gpt-4o-mini"],
    preferredModels: [],
  }),
  "gpt-4.1-mini",
  "key-pool monitor switch should not choose a blocked model",
);

assert.equal(
  selectStationKeyMonitorModel({
    modelAllowlist: [],
    modelBlocklist: [],
    preferredModels: [],
  }),
  "gpt-4.1-mini",
  "key-pool monitor switch should use the current lightweight default model when no explicit model is configured",
);

const createdMonitor = createStationKeyMonitorInput(key, template, capabilities);
assert.equal(createdMonitor.targetType, "station_key");
assert.equal(createdMonitor.stationKeyId, "key-1");
assert.equal(createdMonitor.protocolKind, "open_ai_chat");
assert.equal(createdMonitor.clientProfileId, "standard_api");
assert.equal(createdMonitor.primaryModel, "gpt-4.1-mini");
assert.deepEqual(createdMonitor.fallbackModels, []);
assert.equal(createdMonitor.healthWritebackMode, "observe_only");
assert.equal(createdMonitor.attemptTimeoutMs, 30_000);
assert.equal(createdMonitor.executionTimeoutMs, 45_000);

assert.equal(
  createStationKeyMonitorInput(key, template, capabilities, "codex-auto-review").primaryModel,
  "codex-auto-review",
  "monitor creation should still support an explicitly tested model when one is supplied",
);

const keyPoolPageSource = await readFile("src/features/key-pool/useKeyPoolPageController.ts", "utf8");

assert.ok(
  !keyPoolPageSource.includes("runStationKeyConnectivityOperation") &&
    !keyPoolPageSource.includes("DEFAULT_KEY_CONNECTIVITY_TEST_MODEL") &&
    !keyPoolPageSource.includes("const connectivityResult"),
  "key-pool monitor creation must not wait for a network connectivity operation",
);

assert.ok(
  keyPoolPageSource.includes("createStationKeyMonitorInput(item, preferredTemplate, capabilities)"),
  "key-pool monitor creation should select its initial model from persisted capabilities",
);

assert.ok(
  keyPoolPageSource.includes("useActivityQuery(channelMonitoringQueryOptions())") &&
    keyPoolPageSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.channelMonitoring })") &&
    keyPoolPageSource.includes("queryClient.invalidateQueries({ queryKey: queryKeys.channelStatus })") &&
    !keyPoolPageSource.includes("listChannelMonitors") &&
    !keyPoolPageSource.includes("listChannelMonitorTemplates") &&
    !keyPoolPageSource.includes("refreshMonitorResources"),
  "key-pool monitor resources should be read and refreshed through canonical channel query owners",
);

assert.ok(
  keyPoolPageSource.indexOf("const capabilities = await getStationKeyCapabilities(item.id)")
    < keyPoolPageSource.indexOf("await createChannelMonitor("),
  "key-pool monitor creation should resolve local capabilities before persisting the monitor",
);

const existingMonitor = {
  id: "monitor-1",
  name: "Existing",
  targetType: "station_key",
  stationId: "station-1",
  stationKeyId: "key-1",
  templateId: "template-1",
  enabled: false,
  protocolKind: "open_ai_chat",
  clientProfileId: "standard_api",
  clientProfileVersion: 1,
  primaryModel: "deepseek-chat",
  retryMaxAttemptsPerModel: 2,
  retryInitialBackoffMs: 300,
  retryMaxBackoffMs: 2_000,
  riskDailyProbeBudget: 100,
  healthWritebackMode: "observe_only",
  healthFailureThreshold: 4,
  healthRecoveryThreshold: 2,
  attemptTimeoutMs: 8_000,
  executionTimeoutMs: 20_000,
  intervalSeconds: 120,
  jitterSeconds: 10,
  timeoutSeconds: 20,
  maxConcurrency: 1,
  consecutiveFailureThreshold: 4,
  fallbackModels: ["deepseek-chat"],
  note: null,
  updatedAt: "1000",
};

assert.equal(
  findStationKeyMonitor([existingMonitor], "key-1")?.id,
  "monitor-1",
  "key-pool monitor switch should reuse the synced monitor for the key",
);

const enabledMonitor = updateStationKeyMonitorEnabledInput(existingMonitor, true);
assert.equal(enabledMonitor.enabled, true);
for (const [field, value] of Object.entries(existingMonitor)) {
  if (field !== "enabled" && field !== "updatedAt") {
    assert.deepEqual(enabledMonitor[field], value, `monitor toggle should preserve ${field}`);
  }
}
