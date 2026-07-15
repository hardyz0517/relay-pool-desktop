import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");
const apiSource = await readFile("src/lib/api/channelMonitors.ts", "utf8");
const templateManagerSource = await readFile("src/features/channels/ChannelMonitorTemplateManager.tsx", "utf8");
const monitorFormSource = await readFile("src/features/channels/ChannelMonitorForm.tsx", "utf8");

const builtinSeeder = databaseSource.match(/fn seed_builtin_channel_monitor_templates_in_connection[\s\S]*?fn row_to_channel_monitor_template/)?.[0] ?? "";
assert.ok(builtinSeeder, "database should seed built-in channel monitor templates");

assert.ok(
  builtinSeeder.includes('"stream": "{{stream}}"'),
  "built-in monitor templates should let runtime render stream=true instead of persisting a literal false",
);
assert.ok(
  !builtinSeeder.includes('"instructions": "Reply with OK only."') &&
    !builtinSeeder.includes('"reasoning": { "effort": "minimal" }') &&
    !builtinSeeder.includes('"temperature": 0'),
  "built-in monitor templates should avoid optional fields that Sub2API-compatible streaming probes may reject",
);
assert.ok(
  builtinSeeder.includes('"max_output_tokens": 32'),
  "Responses low-token monitor should leave enough output budget for real streaming terminal events",
);

for (const [name, source] of [
  ["browser fallback templates", apiSource],
  ["custom template defaults", templateManagerSource],
]) {
  assert.ok(
    source.includes('stream: "{{stream}}"'),
    `${name} should render the backend stream flag through the template context`,
  );
  assert.ok(
    !source.includes("stream: false") && !source.includes('instructions: "Reply with OK only."'),
    `${name} should not preserve the old non-streaming/Responses-instructions probe shape`,
  );
}

assert.ok(
  !monitorFormSource.includes("instructions + input"),
  "monitor form copy should describe the current Responses probe shape",
);

console.log("channel monitor template format contract passed");
