import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
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

const { buildChangeEventListItem, markUnreadChangeEventsRead } = await import(pathToFileURL(outFile).href);

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
assert.equal(rateChange.title, "分组 plus 倍率上涨");
assert.deepEqual(rateChange.diff, { label: "倍率", before: "0.7 倍", after: "1 倍" });

const groupAdded = buildChangeEventListItem(
  changeEvent("group-added", "unread", {
    eventType: "group_added",
    title: "分组新增",
    message: "站点新增可用分组 claude-aws",
    newValueJson: JSON.stringify({ groupName: "claude-aws" }),
  }),
);
assert.equal(groupAdded.title, "新增分组 claude-aws");
assert.deepEqual(groupAdded.diff, { label: "分组", before: null, after: "claude-aws" });

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
assert.equal(groupMissing.title, "分组 cmax-限制客户端-暂停供应 不可见");
assert.deepEqual(groupMissing.diff, { label: "状态", before: "可用", after: "不可见" });
