import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const providerConnectionSection = await readFile(
  "src/features/stations/pages/add-provider/AddProviderSections.tsx",
  "utf8",
);
const addProviderController = await readFile(
  "src/features/stations/useAddProviderPageController.ts",
  "utf8",
);
const stationDialogs = await readFile(
  "src/features/stations/pages/stations/StationDialogs.tsx",
  "utf8",
);
const stationDetailPage = await readFile("src/features/stations/StationDetailPage.tsx", "utf8");
const stationsController = await readFile(
  "src/features/stations/useStationsPageController.ts",
  "utf8",
);
const stationFormModel = await readFile(
  "src/features/stations/pages/stations/formModel.ts",
  "utf8",
);
const addKeyPage = await readFile("src/features/key-pool/AddKeyPage.tsx", "utf8");

test("station forms and details keep endpoint roles distinct", () => {
  for (const source of [providerConnectionSection, stationDialogs]) {
    assert.match(source, /websiteUrl/);
    assert.match(source, /apiBaseUrl/);
  }
  assert.match(stationDetailPage, /openStationWebsite\(viewModel\.station\.websiteUrl\)/);
  assert.match(stationsController, /openStationWebsite\(station\.websiteUrl\)/);
  assert.doesNotMatch(addKeyPage, /onChange=.*baseUrl/);
  assert.match(addKeyPage, /stationApiBaseUrl/);
});

test("station endpoint editing exposes copy and origin-change warnings", () => {
  assert.match(providerConnectionSection, /复制前端网址/);
  assert.match(addProviderController, /apiBaseUrl: current\.websiteUrl/);
  assert.match(stationFormModel, /保存的登录状态/);
  assert.match(stationFormModel, /现有密钥将不会路由/);
});
