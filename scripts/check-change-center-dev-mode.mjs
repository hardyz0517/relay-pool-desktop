import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourcePath = resolve(root, "src/features/changes/changeEventViewModels.ts");
const source = readFileSync(sourcePath, "utf8");
const changeCenterSource = readFileSync(resolve(root, "src/features/changes/ChangeCenterPage.tsx"), "utf8");

const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.CommonJS,
    target: ts.ScriptTarget.ES2022,
    esModuleInterop: true,
  },
}).outputText;

const module = { exports: {} };
Function("module", "exports", compiled)(module, module.exports);

const selected = {
  id: "change-collector-recovered",
  stationId: "station-1",
  stationKeyId: null,
  eventType: "collector_recovered",
  severity: "info",
  status: "read",
  objectType: "collector_run",
  objectId: "collector-run-1",
  source: "collector",
  title: "中转站 多幻API 采集恢复",
  message: "站点采集已恢复正常。",
  oldValueJson: null,
  newValueJson: null,
  impactJson: null,
  dedupeKey: "collector_recovered:collector:station-1",
  detectedAt: "2026-07-07T09:46:00.000Z",
  resolvedAt: null,
  createdAt: "2026-07-07T09:46:00.000Z",
  updatedAt: "2026-07-07T09:46:00.000Z",
};

assert.ok(
  !changeCenterSource.includes("InspectorPanel") &&
    !changeCenterSource.includes("变更详情") &&
    !changeCenterSource.includes("JsonBlock") &&
    !changeCenterSource.includes("developerModeEnabled") &&
    !changeCenterSource.includes("buildChangeDetailDescription"),
  "change center should no longer render a separate detail card or developer-mode detail block",
);

const keyInvalid = module.exports.buildChangeEventListItem(
  {
    ...selected,
    id: "change-key-invalid",
    stationId: "station-fylink",
    stationKeyId: "key-fylink-special",
    eventType: "key_invalid",
    severity: "warning",
    status: "unread",
    objectType: "station_key",
    objectId: "key-fylink-special",
    source: "health",
    title: "Key 健康异常",
    message: "Key 连续失败 2 次：Upstream returned HTTP 503",
    newValueJson: JSON.stringify({
      stationKeyName: "FYLinkApi特惠",
      consecutiveFailures: 2,
    }),
  },
  { stationNamesById: new Map([["station-fylink", "FYLinkApi"]]) },
);

assert.equal(
  keyInvalid.description,
  "中转站 FYLinkApi 的 Key FYLinkApi特惠 健康异常：连续失败 2 次；Upstream returned HTTP 503",
);
