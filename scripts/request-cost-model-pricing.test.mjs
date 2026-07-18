import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");
const legacyRuntimeSource = await readFile("src-tauri/src/services/proxy/legacy_runtime.rs", "utf8");
const monitorSource = await readFile("src-tauri/src/services/channel_monitors/mod.rs", "utf8");

assert.match(
  databaseSource,
  /pub fn route_candidate_economics_for_model\([\s\S]*?model: Option<String>[\s\S]*?route_candidate_economics_by_station_key\(&connection, &station_key_id, model\.as_deref\(\)\)/,
  "database should expose a model-aware route economics lookup",
);

assert.match(
  databaseSource,
  /WHEN \?3 IS NOT NULL AND lower\(model\) = lower\(\?3\) THEN 0/,
  "route economics lookup should prefer pricing rules for the requested model",
);

assert.match(
  databaseSource,
  /WHEN input_price IS NOT NULL OR output_price IS NOT NULL OR fixed_price IS NOT NULL THEN 0/,
  "route economics lookup should prefer price-bearing rules over group-rate-only rows",
);

assert.match(
  legacyRuntimeSource,
  /request_cost_for_observed_usage\(\s*context,\s*Some\(&candidate\.station_key_id\),\s*Some\(&candidate\.station_id\),\s*response\.model\.as_deref\(\),\s*&usage,?\s*\)/,
  "proxy request cost extraction should use the actual routed model when choosing pricing",
);

assert.match(
  legacyRuntimeSource,
  /route_candidate_economics_for_model\(\s*station_key_id\.to_string\(\),\s*model\.map\(ToString::to_string\),?\s*\)/,
  "observed usage pricing should forward the routed model to the model-aware economics lookup",
);

assert.match(
  monitorSource,
  /monitor_request_cost\(database, &target\.id, model, usage\.as_ref\(\)\)/,
  "channel monitor request logging should pass the probed model into cost estimation",
);

assert.match(
  monitorSource,
  /route_candidate_economics_for_model\(station_key_id\.to_string\(\), Some\(model\.to_string\(\)\)\)/,
  "channel monitor cost estimation should use model-aware pricing lookup",
);

console.log("request cost model pricing contract passed");
