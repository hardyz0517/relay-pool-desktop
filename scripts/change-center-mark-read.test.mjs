import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const esbuild = require("../node_modules/.pnpm/node_modules/esbuild");

const outFile = resolve(tmpdir(), "relay-pool-change-center-mark-read.test.mjs");
await mkdir(dirname(outFile), { recursive: true });
await esbuild.build({
  entryPoints: ["src/features/changes/changeEventViewModels.ts"],
  outfile: outFile,
  bundle: true,
  platform: "node",
  format: "esm",
  external: ["react", "lucide-react", "@tauri-apps/api/core"],
});

const {
  activeSeverityCount,
  buildChangeEventListItem,
  markUnreadChangeEventsRead,
  paginateChangeEvents,
  unreadChangeCount,
  unreadRiskCount,
} = await import(pathToFileURL(outFile).href);

function changeEvent(id, status, overrides = {}) {
  return {
    id,
    severity: "info",
    eventType: "group_added",
    status,
    title: id,
    message: id,
    objectType: "station",
    objectId: id,
    stationId: id,
    stationKeyId: null,
    pricingRuleId: null,
    requestLogId: null,
    oldValueJson: null,
    newValueJson: null,
    impactJson: null,
    dedupeKey: id,
    source: "test",
    detectedAt: "2026-07-05T00:00:00.000Z",
    resolvedAt: null,
    createdAt: "2026-07-05T00:00:00.000Z",
    updatedAt: "2026-07-05T00:00:00.000Z",
    ...overrides,
  };
}

const calls = [];
const currentEvents = [
  changeEvent("already-read", "read"),
  changeEvent("unread-a", "unread"),
  changeEvent("resolved", "resolved"),
  changeEvent("unread-b", "unread"),
];

const result = await markUnreadChangeEventsRead(currentEvents, async (id) => {
  calls.push(id);
  return { ...currentEvents.find((event) => event.id === id), status: "read" };
});

assert.deepEqual(calls, ["unread-a", "unread-b"], "only unread events should be marked read");
assert.equal(result.changedCount, 2);
assert.deepEqual(
  result.events.map((event) => `${event.id}:${event.status}`),
  ["already-read:read", "unread-a:read", "resolved:resolved", "unread-b:read"],
  "updated events should be merged without changing order or unrelated statuses",
);

const mixedUnreadEvents = [
  changeEvent("risk-warning", "unread", { severity: "warning" }),
  changeEvent("risk-critical", "unread", { severity: "critical" }),
  changeEvent("info-a", "unread", { severity: "info" }),
  changeEvent("info-b", "unread", { severity: "info" }),
  changeEvent("read-info", "read", { severity: "info" }),
];

assert.equal(unreadRiskCount(mixedUnreadEvents), 2, "risk count should still only include unread warning and critical events");
assert.equal(unreadChangeCount(mixedUnreadEvents), 4, "sidebar unread badge should count every unread change event");

const activeSeverityEvents = [
  changeEvent("critical-unread", "unread", { severity: "critical" }),
  changeEvent("critical-read", "read", { severity: "critical" }),
  changeEvent("critical-dismissed", "dismissed", { severity: "critical" }),
  changeEvent("critical-resolved", "resolved", { severity: "critical" }),
  changeEvent("warning-unread", "unread", { severity: "warning" }),
];

assert.equal(typeof activeSeverityCount, "function", "view model should expose the same active severity count used by the page summary");
assert.equal(activeSeverityCount(activeSeverityEvents, "critical"), 2, "active severity count should exclude dismissed and resolved events");
assert.equal(activeSeverityCount(activeSeverityEvents, "warning"), 1);
assert.equal(activeSeverityCount(activeSeverityEvents, "info"), 0);

const pagedEvents = Array.from({ length: 27 }, (_, index) => changeEvent(`event-${index + 1}`, "unread"));
const firstPage = paginateChangeEvents(pagedEvents, 1, 10);
assert.equal(firstPage.page, 1);
assert.equal(firstPage.totalPages, 3);
assert.equal(firstPage.startIndex, 1);
assert.equal(firstPage.endIndex, 10);
assert.deepEqual(
  firstPage.events.map((event) => event.id),
  pagedEvents.slice(0, 10).map((event) => event.id),
  "page one should show only the first page of filtered change events",
);
const lastPage = paginateChangeEvents(pagedEvents, 99, 10);
assert.equal(lastPage.page, 3, "pagination should clamp out-of-range pages after filtering");
assert.equal(lastPage.startIndex, 21);
assert.equal(lastPage.endIndex, 27);
assert.deepEqual(
  lastPage.events.map((event) => event.id),
  pagedEvents.slice(20).map((event) => event.id),
);

const rateChange = buildChangeEventListItem(
  changeEvent("rate-change", "unread", {
    severity: "warning",
    eventType: "rate_changed",
    title: "倍率上涨",
    message: "分组 plus 倍率发生变化",
    stationId: "station-rate",
    oldValueJson: JSON.stringify({ groupName: "plus", multiplier: 0.7 }),
    newValueJson: JSON.stringify({ groupName: "plus", multiplier: 1 }),
  }),
  { stationNamesById: new Map([["station-rate", "倍率站"]]) },
);
assert.equal(rateChange.title, "中转站 倍率站 的分组 plus 倍率从 0.7 倍变为 1 倍");
assert.deepEqual(rateChange.diff, { label: "倍率", before: "0.7 倍", after: "1 倍" });

const groupAdded = buildChangeEventListItem(
  changeEvent("group-added", "unread", {
    eventType: "group_added",
    title: "分组新增",
    message: "站点新增可用分组 claude-aws",
    stationId: "station-blue",
    newValueJson: JSON.stringify({ groupName: "claude-aws" }),
  }),
  { stationNamesById: new Map([["station-blue", "蓝池"]]) },
);
assert.equal(groupAdded.title, "中转站 蓝池 新增分组 claude-aws，倍率未知");
assert.equal(groupAdded.diff, null);

const groupMissing = buildChangeEventListItem(
  changeEvent("group-missing", "unread", {
    severity: "warning",
    eventType: "group_missing",
    title: "分组不可见",
    message: "分组 cmax-限制客户端-暂停供应 在最新采集中不可见",
    stationId: "station-cmax",
    oldValueJson: JSON.stringify({ bindingStatus: "available" }),
    newValueJson: JSON.stringify({ bindingStatus: "missing" }),
  }),
  { stationNamesById: new Map([["station-cmax", "CMax"]]) },
);
assert.equal(groupMissing.title, "中转站 CMax 的分组 cmax-限制客户端-暂停供应 不可见，倍率未知");
assert.deepEqual(groupMissing.diff, { label: "状态", before: "可用", after: "不可见" });

const balanceLow = buildChangeEventListItem(
  changeEvent("balance-low", "unread", {
    severity: "warning",
    eventType: "balance_low",
    title: "余额偏低",
    message: "Orchid Relay 余额低于阈值，可能影响 cheap_first 路由。",
    stationId: "station-orchid",
    newValueJson: JSON.stringify({ value: 4.2, threshold: 10 }),
    source: "balance",
  }),
);
assert.equal(balanceLow.title, "中转站 Orchid Relay 余额偏低：当前 4.2，阈值 10");

const modelAdded = buildChangeEventListItem(
  changeEvent("model-added", "unread", {
    eventType: "model_added",
    title: "模型新增",
    message: "Blue Pool 新增模型 gpt-5-mini。",
    stationId: "station-blue",
    newValueJson: JSON.stringify({ model: "gpt-5-mini" }),
  }),
);
assert.equal(modelAdded.title, "中转站 Blue Pool 新增模型 gpt-5-mini");

const keyInvalidWithName = buildChangeEventListItem(
  changeEvent("key-invalid-named", "unread", {
    severity: "critical",
    eventType: "key_invalid",
    title: "Key 健康异常",
    message: "Key 连续失败 5 次：timeout",
    objectType: "station_key",
    objectId: "key-prod-1",
    stationId: "station-prod",
    stationKeyId: "key-prod-1",
    newValueJson: JSON.stringify({
      stationKeyName: "生产 Key",
      apiKeyMasked: "sk-****prod",
      consecutiveFailures: 5,
    }),
    source: "health",
  }),
  { stationNamesById: new Map([["station-prod", "生产站"]]) },
);
assert.equal(keyInvalidWithName.title, "中转站 生产站 的 Key 生产 Key 健康异常：连续失败 5 次");
assert.equal(keyInvalidWithName.description, "中转站 生产站 的 Key 生产 Key 健康异常：连续失败 5 次；timeout");
assert.equal(keyInvalidWithName.metaLabel, "密钥 / 生产 Key");
assert.deepEqual(keyInvalidWithName.diff, { label: "失败次数", before: null, after: "5 次" });

const keyInvalidWithoutName = buildChangeEventListItem(
  changeEvent("key-invalid-id-only", "unread", {
    severity: "warning",
    eventType: "key_invalid",
    title: "Key 健康异常",
    message: "Key 连续失败 2 次",
    objectType: "station_key",
    objectId: "station-key-abcdef123456",
    stationId: "station-prod",
    stationKeyId: "station-key-abcdef123456",
    newValueJson: JSON.stringify({ consecutiveFailures: 2 }),
    source: "health",
  }),
  { stationNamesById: new Map([["station-prod", "生产站"]]) },
);
assert.equal(keyInvalidWithoutName.title, "中转站 生产站 的 Key station-key-abc... 健康异常：连续失败 2 次");
assert.equal(keyInvalidWithoutName.metaLabel, "密钥 / station-key-abc...");

const changeEventsApiSource = await readFile("src/lib/api/changeEvents.ts", "utf8");
const changeCenterSource = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");
const appShellSource = await readFile("src/components/shell/AppShell.tsx", "utf8");
const mockChangeEventsSource = await readFile("src/lib/mock/changeEvents.ts", "utf8");
const tauriCommandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLibSource = await readFile("src-tauri/src/lib.rs", "utf8");
const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");

assert.ok(
  changeEventsApiSource.includes("CHANGE_EVENTS_UPDATED_EVENT"),
  "change events API should expose a shared browser event name for cross-page refreshes",
);

assert.ok(
  changeCenterSource.includes("notifyChangeEventsUpdated"),
  "change center status actions should notify other surfaces after event state changes",
);

assert.ok(
  changeCenterSource.includes('import { listStations } from "@/lib/api/stations"') &&
    changeCenterSource.includes("stationNamesById") &&
    changeCenterSource.includes("buildChangeEventListItem(event, { stationNamesById })"),
  "change center page should resolve station IDs to station names before rendering event copy",
);

assert.ok(
  appShellSource.includes("CHANGE_EVENTS_UPDATED_EVENT") &&
    appShellSource.includes("window.addEventListener(CHANGE_EVENTS_UPDATED_EVENT") &&
    appShellSource.includes("window.removeEventListener(CHANGE_EVENTS_UPDATED_EVENT"),
  "app shell should refresh the sidebar change badge when change events are updated in-place",
);

assert.ok(
  appShellSource.includes("setInterval(refreshChangeEvents") &&
    appShellSource.includes("clearInterval") &&
    !appShellSource.includes("}, [activeRouteId]);"),
  "app shell should poll change events independently from route changes so backend-created unread events update the badge before opening change center",
);

assert.ok(
  appShellSource.includes("unreadChangeCount(changeEvents)") && !appShellSource.includes("unreadRiskCount(changeEvents)"),
  "app shell badge should use the all-unread count, not the risk-only summary count",
);

assert.ok(
  changeEventsApiSource.includes("clearChangeEvents") &&
    changeEventsApiSource.includes('invoke<void>("clear_change_events"') &&
    changeEventsApiSource.includes("clearMockChangeEvents"),
  "change events API should expose a clear-history command with a mock fallback",
);

assert.ok(
  mockChangeEventsSource.includes("clearMockChangeEvents") && mockChangeEventsSource.includes("memoryChangeEvents = []"),
  "mock change events should support clearing history for browser-only development",
);

assert.ok(
  changeCenterSource.includes("clearChangeHistory") &&
    changeCenterSource.includes("清除记录") &&
    changeCenterSource.includes("pageInfo.events.map") &&
    changeCenterSource.includes("grid-cols-[56px_minmax(0,1fr)_88px]") &&
    !changeCenterSource.includes("function ChangeDiff") &&
    changeCenterSource.includes("上一页") &&
    changeCenterSource.includes("下一页"),
  "change center page should render one-line event rows, a clear-history action, and paginate the filtered event list",
);

assert.ok(
  !changeCenterSource.includes("InspectorPanel") &&
    !changeCenterSource.includes("变更详情") &&
    !changeCenterSource.includes("JsonBlock") &&
    !changeCenterSource.includes("buildChangeDetailDescription"),
  "change center page should remove the separate change-detail card and its raw debug payload body",
);

assert.ok(
  tauriCommandsSource.includes("pub fn clear_change_events") &&
    tauriLibSource.includes("commands::clear_change_events") &&
    databaseSource.includes("pub fn clear_change_events") &&
    databaseSource.includes("DELETE FROM change_events"),
  "Tauri should register a clear_change_events command that deletes persisted change-event history",
);
