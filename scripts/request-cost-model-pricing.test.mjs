import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pricingServiceSource = await readFile("src-tauri/src/application/pricing.rs", "utf8");
const pricingStoreSource = await readFile(
  "src-tauri/src/persistence/stores/pricing_store.rs",
  "utf8",
);
const monitorSource = await readFile("src-tauri/src/services/channel_monitors/mod.rs", "utf8");

assert.match(
  pricingServiceSource,
  /pub\(crate\) async fn resolve_station_key_pricing_context\([\s\S]*?requested_model: &str[\s\S]*?\.resolve_station_key_pricing\(&mut read, station_key_id, requested_model, &now\)/,
  "pricing application service should expose a model-aware station-key pricing lookup",
);

assert.match(
  pricingStoreSource,
  /CASE WHEN lower\(r\.model\) = lower\(\?2\) THEN 0 ELSE 1 END/,
  "route economics lookup should prefer pricing rules for the requested model",
);

assert.match(
  pricingStoreSource,
  /CASE WHEN r\.input_price IS NOT NULL OR r\.output_price IS NOT NULL OR r\.fixed_price IS NOT NULL THEN 0 ELSE 1 END/,
  "route economics lookup should prefer price-bearing rules over group-rate-only rows",
);

assert.match(
  pricingServiceSource,
  /pub\(crate\) async fn estimate_monitor_request_cost\([\s\S]*?requested_model: &str[\s\S]*?\.resolve_station_key_pricing\(&mut read, station_key_id, requested_model, &now\)/,
  "monitor pricing should resolve economics with the probed model",
);

assert.match(
  monitorSource,
  /\.estimate_monitor_request_cost\(&target\.station_key_id, &model, usage\.as_ref\(\)\)/,
  "channel monitor request logging should pass the probed model into cost estimation",
);

console.log("request cost model pricing contract passed");
