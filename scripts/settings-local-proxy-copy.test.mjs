import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settingsPageSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const localProxySectionStart = settingsPageSource.indexOf('title="本地代理"');
const networkSectionStart = settingsPageSource.indexOf('title="网络与代理"');

assert.ok(localProxySectionStart > -1, "settings page should keep the local proxy section");
assert.ok(networkSectionStart > localProxySectionStart, "settings page should keep network settings after local proxy");

const localProxySection = settingsPageSource.slice(localProxySectionStart, networkSectionStart);

assert.ok(
  !localProxySection.includes("description="),
  "local proxy settings rows should not render helper copy under their labels",
);
