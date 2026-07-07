import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";

const monitorSource = await readFile("src-tauri/src/services/channel_monitors/mod.rs", "utf8");
const probeSource = await readFile("src-tauri/src/services/channel_monitors/probe.rs", "utf8");

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
  monitorSource,
  /insert_request_log\(/,
  "successful monitor probes should be recorded in request_logs so dashboard request/token/cost metrics include monitor traffic",
);

assert.match(
  monitorSource,
  /estimated_total_cost/,
  "monitor request log recording should estimate money when pricing is available",
);

console.log("channel monitor usage request-log contract passed");
