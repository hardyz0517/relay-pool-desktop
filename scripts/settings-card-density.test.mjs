import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settingsPageSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");

assert.ok(
  settingsPageSource.includes('contentClassName="p-0"'),
  "settings cards should not add extra vertical padding around setting rows",
);

assert.ok(
  settingsPageSource.includes("description ? \"min-h-14 px-3 py-3\" : \"min-h-12 px-3 py-0\""),
  "settings rows should own the vertical rhythm so first, middle, and last rows align",
);
