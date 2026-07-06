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
  entryPoints: ["src/features/channels/channelMonitorViewModel.ts"],
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

assert.deepEqual(
  createStationKeyMonitorInput(key, template, capabilities),
  {
    name: "Primary Key 监控",
    targetType: "station_key",
    stationId: "station-1",
    stationKeyId: "key-1",
    templateId: "builtin-openai-chat-low-token",
    enabled: true,
    intervalSeconds: 300,
    jitterSeconds: 15,
    timeoutSeconds: 30,
    maxConcurrency: 1,
    consecutiveFailureThreshold: 3,
    fallbackModels: ["gpt-4.1-mini"],
    note: "由密钥池监控开关创建",
  },
  "key-pool monitor switch should create a scheduled station-key monitor",
);

assert.deepEqual(
  createStationKeyMonitorInput(key, template, capabilities, "codex-auto-review").fallbackModels,
  ["codex-auto-review"],
  "first key-pool monitor creation should use the connectivity-tested available model",
);

const keyPoolPageSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");

assert.ok(
  keyPoolPageSource.includes("const connectivityResult = await testStationKeyConnectivity(item.id);"),
  "key-pool monitor creation should identify an actually callable model before creating the monitor",
);

assert.ok(
  /createStationKeyMonitorInput\([^)]*connectivityResult\.model/s.test(keyPoolPageSource),
  "key-pool monitor creation should pass the connectivity-tested model into the default monitor config",
);

const existingMonitor = {
  id: "monitor-1",
  name: "Existing",
  targetType: "station_key",
  stationId: "station-1",
  stationKeyId: "key-1",
  templateId: "template-1",
  enabled: false,
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

assert.deepEqual(
  updateStationKeyMonitorEnabledInput(existingMonitor, true),
  {
    id: "monitor-1",
    name: "Existing",
    targetType: "station_key",
    stationId: "station-1",
    stationKeyId: "key-1",
    templateId: "template-1",
    enabled: true,
    intervalSeconds: 120,
    jitterSeconds: 10,
    timeoutSeconds: 20,
    maxConcurrency: 1,
    consecutiveFailureThreshold: 4,
    fallbackModels: ["deepseek-chat"],
    note: null,
  },
  "key-pool monitor switch should enable the existing monitor without losing schedule settings",
);
