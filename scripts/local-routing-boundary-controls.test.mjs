import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const editor = await readFile("src/features/routing/LocalRoutingSettingsEditor.tsx", "utf8");
const fields = await readFile("src/features/routing/LocalRoutingSettingsFields.tsx", "utf8");
const editTab = await readFile("src/features/routing/LocalRoutingEditTab.tsx", "utf8");

for (const label of [
  "倍率限制",
  "倍率上限",
  "候选分组",
  "默认低余额阈值",
  "余额耗尽兜底",
]) {
  assert.ok(fields.includes(label), `routing boundary should render ${label}`);
}

assert.match(fields, /suffix="×"/);
assert.match(fields, /suffix="CNY"/);
assert.match(fields, /关闭时自动路由不可用/);
assert.match(fields, /站点未单独设置时使用/);

assert.match(editor, /handleBoundarySave/);
assert.match(editor, /保存路由边界/);
assert.match(editor, /eligibleUnderMultiplierLimitCount/);
assert.match(editor, /enabledCandidateCount/);
assert.doesNotMatch(editor, /queueBoundaryAutoSave/);
assert.doesNotMatch(editor, /boundarySaveTimeoutRef/);

assert.match(editTab, /<LocalRoutingSettingsEditor workspace={workspace} \/>/);

console.log("local routing boundary controls contract ok");
