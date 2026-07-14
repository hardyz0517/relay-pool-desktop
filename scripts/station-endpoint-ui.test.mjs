import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const addProvider = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
const stationsPage = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const stationDetails = await readFile("src/features/stations/components/StationDetailPanel.tsx", "utf8");
const addKeyPage = await readFile("src/features/key-pool/AddKeyPage.tsx", "utf8");

test("station forms and details keep endpoint roles distinct", () => {
  for (const source of [addProvider, stationsPage]) {
    assert.match(source, /websiteUrl/);
    assert.match(source, /apiBaseUrl/);
  }
  assert.match(stationDetails, /前端网址/);
  assert.match(stationDetails, /API Base URL/);
  assert.match(stationsPage, /openStationWebsite\(station\.websiteUrl\)/);
  assert.doesNotMatch(addKeyPage, /onChange=.*baseUrl/);
  assert.match(addKeyPage, /stationApiBaseUrl/);
});

test("station endpoint editing exposes copy and origin-change warnings", () => {
  assert.match(addProvider, /复制前端网址/);
  assert.match(addProvider, /apiBaseUrl: current\.websiteUrl/);
  assert.match(stationsPage, /保存的登录状态/);
  assert.match(stationsPage, /现有 Key 将不会路由/);
});
