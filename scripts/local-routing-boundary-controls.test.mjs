import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const readSource = (path) => readFileSync(new URL(`../${path}`, import.meta.url), "utf8");

const fieldsSource = readSource("src/features/routing/LocalRoutingSettingsFields.tsx");
const editorSource = readSource("src/features/routing/LocalRoutingSettingsEditor.tsx");
const editTabSource = readSource("src/features/routing/LocalRoutingEditTab.tsx");

for (const label of [
  "倍率限制",
  "倍率上限",
  "候选分组",
  "默认低余额阈值",
  "余额耗尽兜底",
]) {
  assert.match(fieldsSource, new RegExp(label), `missing boundary label: ${label}`);
}

for (const snippet of [
  'suffix="×"',
  'suffix="CNY"',
  "关闭时自动路由不可用",
  "站点未单独设置时使用",
  "showLabel={false}",
]) {
  assert.ok(fieldsSource.includes(snippet), `missing fields source snippet: ${snippet}`);
}

for (const snippet of [
  "handleBoundarySave",
  "保存路由边界",
  "previewEligibleCandidateCount",
  "candidateCount",
]) {
  assert.ok(editorSource.includes(snippet), `missing editor source snippet: ${snippet}`);
}

for (const removedSnippet of [
  "queueBoundaryAutoSave",
  "boundarySaveTimeoutRef",
  "eligibleUnderMultiplierLimitCount",
  "enabledCandidateCount",
]) {
  assert.ok(
    !editorSource.includes(removedSnippet),
    `editor should not include removed autosave or old summary snippet: ${removedSnippet}`,
  );
}

assert.ok(
  editTabSource.includes("<LocalRoutingSettingsEditor workspace={workspace} />"),
  "LocalRoutingEditTab should pass workspace to LocalRoutingSettingsEditor",
);

console.log("local routing boundary controls source contract ok");
