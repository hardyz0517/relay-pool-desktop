import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settingsPageSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");

assert.ok(
  !settingsPageSource.includes(">保存设置<") &&
    !settingsPageSource.includes("Save className") &&
    !settingsPageSource.includes('form="settings-form"') &&
    !settingsPageSource.includes("handleSubmit"),
  "settings page should not expose a global save-settings button or submit handler",
);

assert.ok(
  settingsPageSource.includes("commitSettingsForm") &&
    settingsPageSource.includes("onCommit") &&
    settingsPageSource.includes("handleCollectorProxyModeChange") &&
    settingsPageSource.includes("handleAllowDepletedFallbackToggle"),
  "settings page should autosave individual setting changes",
);
