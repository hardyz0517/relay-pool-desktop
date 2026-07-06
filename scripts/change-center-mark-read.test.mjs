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

const { buildChangeEventListItem, markUnreadChangeEventsRead, unreadChangeCount, unreadRiskCount } = await import(
  pathToFileURL(outFile).href
);

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

const rateChange = buildChangeEventListItem(
  changeEvent("rate-change", "unread", {
    severity: "warning",
    eventType: "rate_changed",
    title: "倍率上涨",
    message: "分组 plus 倍率发生变化",
    oldValueJson: JSON.stringify({ groupName: "plus", multiplier: 0.7 }),
    newValueJson: JSON.stringify({ groupName: "plus", multiplier: 1 }),
  }),
);
assert.equal(rateChange.title, "分组 plus 倍率变化");
assert.deepEqual(rateChange.diff, { label: "倍率", before: "0.7 倍", after: "1 倍" });

const groupAdded = buildChangeEventListItem(
  changeEvent("group-added", "unread", {
    eventType: "group_added",
    title: "分组新增",
    message: "站点新增可用分组 claude-aws",
    newValueJson: JSON.stringify({ groupName: "claude-aws" }),
  }),
);
assert.equal(groupAdded.title, "新增可用分组 claude-aws，倍率 未知");
assert.equal(groupAdded.diff, null);

const groupMissing = buildChangeEventListItem(
  changeEvent("group-missing", "unread", {
    severity: "warning",
    eventType: "group_missing",
    title: "分组不可见",
    message: "分组 cmax-限制客户端-暂停供应 在最新采集中不可见",
    oldValueJson: JSON.stringify({ bindingStatus: "available" }),
    newValueJson: JSON.stringify({ bindingStatus: "missing" }),
  }),
);
assert.equal(groupMissing.title, "分组 cmax-限制客户端-暂停供应 不可见，倍率 未知");
assert.deepEqual(groupMissing.diff, { label: "状态", before: "可用", after: "不可见" });

const keyInvalidWithName = buildChangeEventListItem(
  changeEvent("key-invalid-named", "unread", {
    severity: "critical",
    eventType: "key_invalid",
    title: "Key 健康异常",
    message: "Key 连续失败 5 次：timeout",
    objectType: "station_key",
    objectId: "key-prod-1",
    stationKeyId: "key-prod-1",
    newValueJson: JSON.stringify({
      stationKeyName: "生产 Key",
      apiKeyMasked: "sk-****prod",
      consecutiveFailures: 5,
    }),
    source: "health",
  }),
);
assert.equal(keyInvalidWithName.title, "Key 生产 Key 健康异常");
assert.equal(keyInvalidWithName.description, "sk-****prod 连续失败 5 次：timeout");
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
    stationKeyId: "station-key-abcdef123456",
    newValueJson: JSON.stringify({ consecutiveFailures: 2 }),
    source: "health",
  }),
);
assert.equal(keyInvalidWithoutName.title, "Key station-key-abc... 健康异常");
assert.equal(keyInvalidWithoutName.metaLabel, "密钥 / station-key-abc...");

const changeEventsApiSource = await readFile("src/lib/api/changeEvents.ts", "utf8");
const changeCenterSource = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");
const appShellSource = await readFile("src/components/shell/AppShell.tsx", "utf8");

assert.ok(
  changeEventsApiSource.includes("CHANGE_EVENTS_UPDATED_EVENT"),
  "change events API should expose a shared browser event name for cross-page refreshes",
);

assert.ok(
  changeCenterSource.includes("notifyChangeEventsUpdated"),
  "change center status actions should notify other surfaces after event state changes",
);

assert.ok(
  appShellSource.includes("CHANGE_EVENTS_UPDATED_EVENT") &&
    appShellSource.includes("window.addEventListener(CHANGE_EVENTS_UPDATED_EVENT") &&
    appShellSource.includes("window.removeEventListener(CHANGE_EVENTS_UPDATED_EVENT"),
  "app shell should refresh the sidebar change badge when change events are updated in-place",
);

assert.ok(
  appShellSource.includes("unreadChangeCount(changeEvents)") && !appShellSource.includes("unreadRiskCount(changeEvents)"),
  "app shell badge should use the all-unread count, not the risk-only summary count",
);
