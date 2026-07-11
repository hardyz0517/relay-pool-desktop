import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settingsPageSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const localProxySectionStart = settingsPageSource.indexOf('title="本地代理"');
const routingSectionStart = settingsPageSource.indexOf('title="采集与路由"');

assert.ok(localProxySectionStart > -1, "settings page should keep the local proxy section");
assert.ok(routingSectionStart > localProxySectionStart, "settings page should keep routing after local proxy");

const localProxySection = settingsPageSource.slice(localProxySectionStart, routingSectionStart);

assert.ok(
  !localProxySection.includes("description="),
  "local proxy settings rows should not render helper copy under their labels",
);
