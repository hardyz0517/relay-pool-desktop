import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const files = {
  collectors: "src-tauri/src/application/collectors.rs",
  collectorHistory: "src-tauri/src/application/queries/collector_history.rs",
  collectorStore: "src-tauri/src/persistence/stores/collector_store.rs",
  collectorApply: "src-tauri/src/services/collectors/collector_apply.rs",
  collectorModule: "src-tauri/src/services/collectors/mod.rs",
  stationCatalog: "src-tauri/src/persistence/stores/station_catalog.rs",
  pricingStore: "src-tauri/src/persistence/stores/pricing_store.rs",
  alertingUpgrade: "src-tauri/src/services/data_store/alerting_upgrade.rs",
  assetRevisionStore: "src-tauri/src/persistence/stores/asset_revision_store.rs",
  revisionNotices: "src-tauri/src/application/queries/read_model_revision.rs",
  domainMapping: "src/lib/bridge/domainMapping.ts",
  stationDialogs: "src/features/stations/pages/stations/StationDialogs.tsx",
  stationAssetRows: "src/features/stations/pages/stations/StationAssetRows.tsx",
  stationDetailViewModels: "src/features/stations/stationDetailViewModels.ts",
  stationDetailQuery: "src-tauri/src/application/queries/station_detail.rs",
  balanceFacts: "src/lib/projections/balanceFacts.ts",
  stationDisplayModel: "src/features/stations/pages/stations/displayModel.ts",
};

const sources = Object.fromEntries(
  await Promise.all(Object.entries(files).map(async ([key, path]) => [key, await readFile(path, "utf8")]))
);

// Rust test modules intentionally retain compatibility fixtures. Keep
// architecture checks focused on production code so fixtures cannot mask a
// forbidden dependency or make a valid migration fail.
function productionSource(source) {
  const testModule = /\n#\[cfg\(test\)\]\s*mod\s+tests\s*\{/m.exec(source);
  return testModule ? source.slice(0, testModule.index) : source;
}

const productionCollectors = productionSource(sources.collectors);
const productionCollectorStore = productionSource(sources.collectorStore);
const collectionState = await readFile("src-tauri/src/application/collection_state.rs", "utf8");
const productionCollectorHistory = productionSource(sources.collectorHistory);

assert.match(
  productionCollectorHistory,
  /struct CollectorHistoryQuery[\s\S]{0,1200}impl CollectorHistoryQuery/,
  "collector history must have an explicit read-only query owner",
);
assert.equal(
  /begin_write|WriteSession|\b(?:INSERT\s+INTO|UPDATE\s+\w+|DELETE\s+FROM)\b/i.test(
    productionCollectorHistory,
  ),
  false,
  "collector history query must remain read-only",
);
assert.match(
  sources.stationDetailQuery,
  /collector_history\s*\.\s*list_collector_runs_in_session[\s\S]{0,220}collector_history\s*\.\s*latest_station_snapshot_in_session/,
  "Station Detail must consume collector history through its query owner",
);
for (const method of [
  "list_collector_runs",
  "list_station_snapshots",
  "latest_station_snapshot",
  "list_latest_station_snapshots",
]) {
  assert.equal(
    new RegExp(`pub\\(crate\\) async fn ${method}\\b`).test(productionCollectors),
    false,
    `CollectorService must not own collector history query ${method}`,
  );
  assert.match(
    productionCollectorHistory,
    new RegExp(`pub\\(crate\\) async fn ${method}\\b`),
    `CollectorHistoryQuery must expose ${method}`,
  );
}

assert.equal(
  /set_station_collection_status_direct|update_station_collection_status|aggregate_station_collection_status|project_station_collection_status|station_collection_status_for_request/.test(
    productionCollectors,
  ),
  false,
  "collector application must not retain a legacy station-status owner",
);
assert.equal(
  /set_station_collection_status_direct|assert_collection_intent_watermark|SELECT\s+updated_at_ms\s+FROM\s+station_collection_projection/.test(
    productionCollectorStore,
  ),
  false,
  "collector persistence must use typed intent sequence rather than a timestamp watermark",
);
assert.equal(
  /full_projection_watermark_ms|projection_watermark_ms/.test(productionCollectors),
  false,
  "collector application must not use execution timestamps as a collection authority fence",
);
assert.match(
  productionCollectors,
  /assert_station_collection_intent\([\s\S]{0,240}request\.intent_sequence/,
  "collector terminal apply must validate the durable typed intent sequence",
);
assert.match(
  productionCollectorStore,
  /projection\.intent_sequence\s*<\s*current_intent_sequence/,
  "collection projection persistence must reject an older typed intent sequence",
);
assert.equal(
  /parent_run_id\.is_some\(\)[\s\S]{0,220}(?:authorization_expired|record_in_session)/.test(
    productionCollectors,
  ),
  false,
  "authorization side effects must use an explicit commit policy, not the historical parent run relationship",
);
assert.equal(
  /request\.parent_run_id|child\.parent_run_id|parent\.parent_run_id/.test(productionCollectors),
  false,
  "collector control flow must not branch on historical parent_run_id",
);
assert.equal(
  /apply_station_output\([\s\S]{0,520}parent_run_id|run_key_for_current_intent\([\s\S]{0,320}parent_run_id/.test(
    sources.collectorApply,
  ),
  false,
  "collector request/key builder must not accept or derive parent_run_id",
);
assert.match(
  sources.collectors,
  /#\[cfg\(test\)\][\s\S]{0,180}#\[serde\(skip\)\][\s\S]{0,80}pub parent_run_id: Option<String>/,
  "historical parent linkage may remain only on test compatibility requests and must not affect canonical hashes",
);

const legacyApplyAndRouteSymbols = [
  "V2CollectorApplyAdapter",
  "apply_station_output_v2",
  "apply_prepared_full_collection_v2",
  "apply_prepared_station_collection_v2",
  "apply_prepared_station_task_v2",
  "PreparedStationTaskRoute",
  "prepare_station_task_route_v2",
  "prepare_station_collection_route_v2",
];
const collectorProduction = `${sources.collectorApply}\n${sources.collectorModule}`;
for (const symbol of legacyApplyAndRouteSymbols) {
  assert.equal(
    collectorProduction.includes(symbol),
    false,
    `collector production must not retain legacy apply/route symbol ${symbol}`,
  );
}
assert.match(
  sources.collectorApply,
  /impl CollectorApplyPort for CollectorService/,
  "the canonical CollectorService must implement the atomic apply port directly",
);
assert.equal(
  [...sources.collectorModule.matchAll(/fn prepare_station_collection_route\b/g)].length,
  1,
  "provider routing must have exactly one production helper",
);

assert.equal(
  /UPDATE\s+stations[\s\S]{0,1400}?\bstatus\s*=|INSERT\s+INTO\s+stations\s*\([^)]*\bstatus\b/i.test(
    sources.stationCatalog,
  ),
  false,
  "station catalog production writes must not touch stations.status",
);
assert.equal(
  /\bs\.status\b|SELECT\s+id\s*,\s*status\s*,\s*updated_at\s+FROM\s+stations/i.test(
    `${sources.pricingStore}\n${sources.alertingUpgrade}`,
  ),
  false,
  "production queries must not read stations.status",
);
assert.match(
  sources.alertingUpgrade,
  /JOIN endpoint_health_snapshot endpoint_health[\s\S]{0,240}endpoint_health\.endpoint_revision = stations\.endpoint_revision/,
  "legacy alert rebuilding must use endpoint-revision-fenced health evidence",
);
assert.equal(
  sources.stationCatalog.includes("collector_task_state"),
  false,
  "station scheduling must not read the legacy collector task projection",
);
assert.equal(
  sources.alertingUpgrade.includes("collector_task_state"),
  false,
  "alerting current-fact rebuild must not read the legacy collector task projection",
);
assert.equal(
  /\.update_task_state\s*\(/.test(productionCollectors),
  false,
  "collector terminal apply must not write the legacy collector task projection",
);
assert.equal(
  /#\[cfg\(test\)\][\s\S]{0,240}pub\(crate\) async fn update_task_state_for_test/.test(
    sources.collectorStore,
  ),
  true,
  "legacy task projection writer remains available only for compatibility tests",
);
assert.equal(
  /(?:serde_json::|\bValue\b|persistence::|tauri::)/.test(collectionState),
  false,
  "collection reducer must remain a pure typed function without persistence/JSON/Tauri dependencies",
);
assert.equal(
  sources.domainMapping.includes("station.status"),
  false,
  "frontend transport normalization must not read the retired station status rollup",
);

assert.equal(
  /MAX\s*\(\s*revision\s*\)/i.test(sources.assetRevisionStore),
  false,
  "asset revision reads must not use MAX over independent scopes",
);
assert.match(
  sources.revisionNotices,
  /revision_vector/,
  "revision notices must retain a typed scope revision vector",
);
assert.equal(
  sources.stationDialogs.includes("activeDialogStation.status"),
  false,
  "station details must use typed collection/authorization summaries",
);
assert.match(
  sources.stationDialogs,
  /formatStationStatusLabel\(activeDialogStation\)/,
  "station details must render collection status through the typed view model",
);
assert.equal(
  /(?:station\.)?last(?:Checked|PricingFetched)At/.test(sources.stationAssetRows),
  false,
  "station asset rows must not use compatibility freshness fields as current activity",
);
assert.equal(
  /station\.last(?:Checked|PricingFetched)At/.test(sources.stationDetailViewModels),
  false,
  "station detail view model must not use compatibility freshness fields as current activity",
);
assert.equal(
  /station\.last(?:Checked|PricingFetched)At|source:\s*['\"]station_cache/.test(sources.balanceFacts),
  false,
  "balance projection must fail closed instead of falling back to station compatibility cache",
);
assert.equal(
  /activeDialogStation\.last(?:Checked|PricingFetched)At/.test(sources.stationDialogs),
  false,
  "station dialog must render typed collector history rather than compatibility timestamps",
);
assert.equal(
  /row\.station\.balanceCny|station\.balanceCny/.test(sources.stationDisplayModel),
  false,
  "station display formatting must use typed balance facts, not the compatibility station cache",
);

console.log("station collection authority checks passed");
