import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";

const monitorSource = await readFile("src-tauri/src/services/channel_monitors/mod.rs", "utf8");
const probeSource = await readFile("src-tauri/src/services/channel_monitors/probe.rs", "utf8");
const modelSource = await readFile("src-tauri/src/models/channel_monitors.rs", "utf8");
const applicationSource = await readFile("src-tauri/src/application/monitoring.rs", "utf8");
const requestLogStoreSource = await readFile(
  "src-tauri/src/persistence/stores/request_log_store.rs",
  "utf8",
);

assert.match(
  probeSource,
  /pub struct MonitorProbeUsage/,
  "monitor probe should expose parsed token usage from successful upstream responses",
);

assert.match(
  probeSource,
  /prompt_tokens/,
  "monitor probe usage parsing should preserve prompt token counts",
);

assert.match(
  modelSource,
  /struct CompletedMonitorProbe/,
  "monitor probes should cross the application boundary as completed domain evidence",
);

assert.match(
  monitorSource,
  /estimate_monitor_request_cost/,
  "monitor probes should resolve pricing through the pricing application service",
);

assert.match(
  applicationSource,
  /insert_run_and_advance_monitor[\s\S]*insert_completed_monitor_observation/,
  "monitor run and request-log evidence should be written in one application transaction",
);

assert.match(
  requestLogStoreSource,
  /prompt_tokens[\s\S]*reasoning_effort[\s\S]*first_token_ms[\s\S]*estimated_total_cost/,
  "completed monitor request logs should preserve usage, reasoning, latency, and cost evidence",
);

assert.doesNotMatch(
  requestLogStoreSource,
  /"source": "channel_monitor"|\.bind\(record\.error_message/,
  "monitor diagnostics should stay in monitor runs instead of generic request-log fields",
);

console.log("channel monitor usage request-log contract passed");
