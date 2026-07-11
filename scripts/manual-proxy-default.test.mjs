import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const helper = readFileSync("src/lib/proxyDefaults.ts", "utf8");
const settingsPage = readFileSync("src/features/settings/SettingsPage.tsx", "utf8");
const addProviderPage = readFileSync("src/features/stations/AddProviderPage.tsx", "utf8");

assert.match(
  helper,
  /DEFAULT_MANUAL_PROXY_URL\s*=\s*"http:\/\/127\.0\.0\.1:7890"/,
  "manual proxy default should be the local 7890 HTTP proxy",
);
assert.match(
  helper,
  /withManualProxyDefault/,
  "manual proxy default helper should be centralized",
);

for (const [label, source] of [
  ["SettingsPage", settingsPage],
  ["AddProviderPage", addProviderPage],
]) {
  assert.ok(
    source.includes("DEFAULT_MANUAL_PROXY_URL"),
    `${label} should use the shared 7890 placeholder/default`,
  );
  assert.ok(
    source.includes("withManualProxyDefault"),
    `${label} should fill an empty manual proxy URL when switching to manual mode`,
  );
}
