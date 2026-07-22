import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const templateRendererSource = await readFile(
  "src-tauri/src/services/channel_monitors/templates.rs",
  "utf8",
);
const apiSource = await readFile("src/lib/api/channelMonitors.ts", "utf8");
const templateManagerSource = await readFile("src/features/channels/ChannelMonitorTemplateManager.tsx", "utf8");
const monitorFormSource = await readFile("src/features/channels/ChannelMonitorForm.tsx", "utf8");

assert.ok(
  templateRendererSource.includes('"{{stream}}" => Value::Bool(context.stream)') &&
    templateRendererSource.includes('.replace("{{stream}}", if context.stream { "true" } else { "false" })'),
  "monitor template rendering should preserve typed stream placeholders in JSON and mixed strings",
);
assert.ok(
  !apiSource.includes('instructions: "Reply with OK only."') &&
    !apiSource.includes('reasoning: { effort: "minimal" }') &&
    !apiSource.includes("temperature: 0"),
  "preview monitor templates should avoid optional fields that Sub2API-compatible streaming probes may reject",
);
assert.ok(
  apiSource.includes("max_output_tokens: 32"),
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
