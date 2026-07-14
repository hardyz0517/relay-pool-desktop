import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const stationTypes = await readFile("src/lib/types/stations.ts", "utf8");
const stationApi = await readFile("src/lib/api/stations.ts", "utf8");
const keyTypes = await readFile("src/lib/types/stationKeys.ts", "utf8");
const presets = await readFile("src/features/stations/providerPresets.ts", "utf8");
const pricing = await readFile("src/features/pricing/PricingPage.tsx", "utf8");
const collectors = await readFile("src/features/collectors/CollectorsPage.tsx", "utf8");
const dashboard = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const keyPool = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const runtimeSnapshot = await readFile("src/lib/projections/runtimeSnapshot.ts", "utf8");

test("station contracts expose explicit website and API fields", () => {
  assert.match(stationTypes, /websiteUrl: string/);
  assert.match(stationTypes, /apiBaseUrl: string/);
  assert.match(stationTypes, /endpointRevision: number/);
  assert.doesNotMatch(stationTypes, /^\s*baseUrl: string/m);
  assert.match(keyTypes, /stationApiBaseUrl: string/);
  assert.doesNotMatch(keyTypes, new RegExp("station" + "BaseUrl: string"));
});

test("memory fallback and presets carry both endpoint roles", () => {
  assert.match(stationApi, /websiteUrl: input\.websiteUrl/);
  assert.match(stationApi, /apiBaseUrl: input\.apiBaseUrl/);
  assert.match(presets, /websiteUrl: string/);
  assert.match(presets, /apiBaseUrl: string/);
});

test("views and runtime projections consume the correct endpoint role", () => {
  assert.match(pricing, /openStationWebsite/);
  assert.match(pricing, /websiteUrl/);
  assert.match(collectors, /websiteUrl/);
  assert.match(dashboard, /stationApiBaseUrl/);
  assert.match(keyPool, /stationApiBaseUrl/);
  assert.match(runtimeSnapshot, /upstreamBaseUrl: station\.apiBaseUrl/);
  assert.match(runtimeSnapshot, /endpointRevision: station\.endpointRevision/);
});
