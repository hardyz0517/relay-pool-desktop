import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";

function read(path) {
  return existsSync(path) ? readFileSync(path, "utf8") : "";
}

const editTab = read("src/features/routing/LocalRoutingEditTab.tsx");
const editor = read("src/features/routing/LocalRoutingSettingsEditor.tsx");
const fields = read("src/features/routing/LocalRoutingSettingsFields.tsx");
const form = read("src/features/routing/localRoutingSettingsForm.ts");
const settingsApi = read("src/lib/api/settings.ts");
const settingsTypes = read("src/lib/types/settings.ts");
const settingsPage = read("src/features/settings/SettingsPage.tsx");

assert.match(editTab, /LocalRoutingSettingsEditor/);
assert.match(editor, /getSettings/);
assert.match(editor, /updateSettings/);
assert.match(editor, /SETTINGS_UPDATED_EVENT/);
assert.match(editor, /appSettingsToUpdateInput/);
assert.match(editor, /window\.dispatchEvent\(new Event\(SETTINGS_UPDATED_EVENT\)\)/);
assert.doesNotMatch(editor, /@tauri-apps\/api|\binvoke\s*\(/);

const editSurface = editTab + editor + fields;
assert.match(editSurface, /倍率上限/);
assert.match(editSurface, /候选分组/);
assert.match(editor, /保存设置/);
assert.match(editor, /恢复默认/);
assert.doesNotMatch(fields, /无候选策略|automatic_balanced|严格拒绝/);
assert.doesNotMatch(editSurface, /运行时会综合|分组筛选不会跨组兜底|当前仅展示.*骨架/);
assert.match(editor, /boundarySaveState/);
assert.match(editor, /schedulerSaveState/);
assert.match(editor, /schedulerDirty/);
assert.match(editor, /handleBoundarySave/);
assert.match(editor, /settingsRef/);
assert.match(editor, /updateBoundaryNumericField/);
assert.match(editor, /parseLocalRoutingBoundaryDraft/);
assert.match(editor, /parseLocalRoutingSchedulerDraft/);
assert.match(
  editor,
  /schedulerAdvancedSettings:\s*\{[\s\S]*currentSettings\.schedulerAdvancedSettings[\s\S]*parsed\.value\.schedulerAdvancedPatch[\s\S]*\}/,
  "boundary save must merge boundary scheduler patch into the latest saved scheduler settings",
);
assert.match(
  editor,
  /const schedulerDisabled = loading \|\| schedulerSaveState === "saving" \|\| boundarySaveState === "saving";/,
  "scheduler save must be disabled while a boundary save is pending",
);
assert.match(
  editor,
  /const boundaryDisabled = loading \|\| schedulerSaveState === "saving" \|\| boundarySaveState === "saving";/,
  "boundary save must be disabled while a scheduler save is pending",
);
assert.match(
  editor,
  /onNumericChange=\{updateBoundaryNumericField\}/,
  "boundary numeric fields must use boundary-specific parsing and save state",
);

const schedulerFields = [
  "topK",
  "multiplier",
  "priority",
  "load",
  "queue",
  "errorRate",
  "ttft",
  "quotaHeadroom",
  "previousResponse",
  "sessionSticky",
  "multiplierMinConfidence",
  "stickyWeighted",
  "stickyEscape",
  "stickyEscapeTtftMs",
  "stickyEscapeErrorRate",
  "stickySessionTtlSeconds",
  "stickyResponseTtlSeconds",
  "stickyMaxWaiting",
  "stickyWaitTimeoutSeconds",
  "fallbackMaxWaiting",
  "fallbackWaitTimeoutSeconds",
];

for (const field of schedulerFields) {
  assert.match(settingsTypes, new RegExp(`${field}:`), `settings schema must cover ${field}`);
  if (field !== "stickyEscape") {
    assert.match(form, new RegExp(`${field}:`), `form metadata must cover ${field}`);
  }
}

assert.match(settingsTypes, /SCHEDULER_ADVANCED_FIELD_KINDS/);
assert.match(settingsTypes, /satisfies Record<keyof SchedulerAdvancedSettings, SchedulerAdvancedFieldKind>/);
assert.match(settingsTypes, /appSettingsToUpdateInput/);
assert.match(form, /createLocalRoutingSettingsDraft/);
assert.match(form, /parseLocalRoutingSettingsDraft/);
assert.match(form, /Number\.isSafeInteger/);
assert.match(form, /topK.*65_535/s);
assert.match(form, /baseWeights/);
assert.match(form, /multiplierMinConfidence/);
assert.match(form, /stickyEscapeErrorRate/);
assert.doesNotMatch(
  form,
  /stickyEscape:\s*\{\s*label:/,
  "sticky escape is an internal default-on safeguard and must not render as a user switch",
);

const promotedStickyIndex = fields.indexOf('field="stickyWeighted"');
const scoreGroupIndex = fields.indexOf('title="综合评分"');
assert.ok(promotedStickyIndex >= 0, "stickyWeighted must render as an explicit promoted row");
assert.ok(
  promotedStickyIndex < scoreGroupIndex,
  "stickyWeighted must render above the score parameter group",
);
assert.match(fields, /PROMOTED_BOOLEAN_FIELDS[\s\S]*stickyWeighted/);
assert.doesNotMatch(fields, /field="stickyEscape"/);
assert.match(fields, /!PROMOTED_BOOLEAN_FIELDS\.has\(field\)/);
assert.match(
  fields,
  /function PromotedBooleanSettingRow[\s\S]*showLabel=\{false\}[\s\S]*border-0[\s\S]*bg-transparent/,
);
assert.doesNotMatch(fields, /<legend/);
assert.match(fields, /role="group"[\s\S]*aria-label=\{title\}/);
assert.match(fields, /<h3 className="mb-3 text-xs font-semibold text-slate-700">\{title\}<\/h3>/);

assert.match(settingsApi, /normalizeSchedulerAdvancedSettings/);
assert.match(settingsApi, /SCHEDULER_ADVANCED_FIELD_KINDS/);
assert.match(settingsApi, /DEFAULT_SCHEDULER_ADVANCED_SETTINGS/);
assert.doesNotMatch(
  settingsPage,
  /["'][^"'\r\n]*\?{3,}[^"'\r\n]*["']/,
  "settings routing copy must not contain corrupted question-mark strings",
);

console.log("local routing smart edit contract ok");
