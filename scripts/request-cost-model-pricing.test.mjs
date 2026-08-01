import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pricingServiceSource = await readFile("src-tauri/src/application/pricing.rs", "utf8");
const pricingProjectorSource = await readFile(
  "src-tauri/src/application/operational_facts/pricing_projector.rs",
  "utf8",
);
const pricingStoreSource = await readFile(
  "src-tauri/src/persistence/stores/pricing_store.rs",
  "utf8",
);

assert.match(
  pricingServiceSource,
  /pub\(crate\) async fn resolve_station_key_pricing_context\([\s\S]*?requested_model: &str[\s\S]*?\.resolve_station_key_pricing\(&mut read, station_key_id, requested_model, &now\)/,
  "pricing application service should expose a model-aware station-key pricing lookup",
);

assert.match(
  pricingServiceSource,
  /operational_facts::pricing_projector::pricing_context_from_resolution/,
  "pricing application service should reuse the operational pricing projector instead of owning duplicated projection logic",
);

assert.match(
  pricingProjectorSource,
  /pub\(crate\) fn pricing_context_from_resolution\(/,
  "operational pricing projector should own the canonical pricing context projection",
);

assert.match(
  pricingProjectorSource,
  /pub\(crate\) fn request_cost_comparison_context\(/,
  "operational pricing projector should expose routing cost-basis classification",
);

assert.match(
  pricingProjectorSource,
  /pub\(crate\) source_chain: Vec<String>/,
  "routing cost comparison context should freeze the pricing evidence source chain",
);

assert.match(
  pricingProjectorSource,
  /pub\(crate\) observed_at: Option<String>/,
  "routing cost comparison context should carry pricing freshness",
);

assert.match(
  pricingProjectorSource,
  /pub\(crate\) confidence: Option<f64>/,
  "routing cost comparison context should carry pricing confidence",
);

assert.match(
  pricingProjectorSource,
  /RoutingCostBasis::MultiplierProxy/,
  "cost-first routing should distinguish multiplier proxy from exact prices",
);

assert.match(
  pricingProjectorSource,
  /PricingRouteKind::ModelCatalog/,
  "model catalog routes should be represented as an explicit non-request-cost route kind",
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

console.log("request cost model pricing contract passed");
