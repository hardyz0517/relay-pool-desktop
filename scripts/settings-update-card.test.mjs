import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settingsPageSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");

assert.ok(
  settingsPageSource.includes("UpdateSettingsCard") &&
    settingsPageSource.includes("Relay Pool") &&
    settingsPageSource.includes("版本") &&
    settingsPageSource.includes("GitHub") &&
    settingsPageSource.includes("更新日志") &&
    settingsPageSource.includes("currentReleaseUrl") &&
    settingsPageSource.includes("/releases/tag/v"),
  "settings update section should render a product-style update card with GitHub and current-version release log actions",
);

assert.ok(
  !settingsPageSource.includes("updaterStatusDescription(state)") &&
    !settingsPageSource.includes("正在检查 GitHub Releases"),
  "settings update card should not render update status copy inside the card",
);

assert.ok(
  !settingsPageSource.includes("官方网站") &&
    !settingsPageSource.includes("官网"),
  "settings update card should not include an official website action",
);
