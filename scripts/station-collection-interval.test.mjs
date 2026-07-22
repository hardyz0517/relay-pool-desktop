import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
const stationsPageSource = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const stationTypesSource = await readFile("src/lib/types/stations.ts", "utf8");
const stationApiSource = await readFile("src/lib/api/stations.ts", "utf8");
const rustStationModelSource = await readFile("src-tauri/src/models/stations.rs", "utf8");
const stationCatalogSource = await readFile(
  "src-tauri/src/persistence/stores/station_catalog.rs",
  "utf8",
);
const catalogMigrationSource = await readFile(
  "src-tauri/src/persistence/migrations/0002_catalog_settings.sql",
  "utf8",
);

[
  ["AddProviderPage form state", addProviderSource],
  ["StationsPage form state", stationsPageSource],
  ["Station TypeScript types", stationTypesSource],
].forEach(([label, source]) => {
  assert.ok(
    source.includes("collectionIntervalMinutes"),
    `${label} should include the station collection interval field`,
  );
});

assert.ok(
  addProviderSource.includes('collectionIntervalMinutes: "5"'),
  "new supplier forms should default collection interval to 5 minutes",
);

assert.ok(
  addProviderSource.includes("station.collectionIntervalMinutes"),
  "editing a supplier should hydrate the collection interval from the saved station",
);

assert.ok(
  addProviderSource.includes('Field label="采集频率 分钟"') ||
    addProviderSource.includes('Field label="閲囬泦棰戠巼 鍒嗛挓"'),
  "supplier optional settings should render a collection frequency minutes field",
);

assert.ok(
  addProviderSource.includes("normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes)"),
  "supplier submit payload should normalize the collection interval field",
);

assert.ok(
  stationsPageSource.includes("normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes)"),
  "station dialog submit payload should normalize the collection interval field",
);

assert.ok(
  stationApiSource.includes("collectionIntervalMinutes: input.collectionIntervalMinutes"),
  "browser preview station API fallback should persist collection interval minutes",
);

assert.ok(
  rustStationModelSource.includes("pub collection_interval_minutes: u16"),
  "Rust station model should expose collection interval minutes",
);

assert.ok(
  catalogMigrationSource.includes("collection_interval_minutes INTEGER NOT NULL") &&
    catalogMigrationSource.includes("CHECK (collection_interval_minutes > 0)"),
  "V2 SQLite schema should require a positive station collection interval",
);

assert.ok(
  stationCatalogSource.includes("collection_interval_minutes = ?13") &&
    stationCatalogSource.includes("collection_interval_minutes == 0"),
  "station catalog writes should persist and validate the collection interval",
);
