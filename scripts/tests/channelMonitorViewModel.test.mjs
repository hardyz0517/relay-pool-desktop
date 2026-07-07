import assert from "node:assert/strict";
import { Buffer } from "node:buffer";
import { readFile } from "node:fs/promises";
import ts from "typescript";

const timeSource = await readFile(new URL("../../src/lib/time.ts", import.meta.url), "utf8");
const compiledTime = ts.transpileModule(timeSource, {
  compilerOptions: {
    jsx: ts.JsxEmit.ReactJSX,
    module: ts.ModuleKind.ES2022,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText;
const timeModuleUrl = `data:text/javascript;base64,${Buffer.from(compiledTime).toString("base64")}`;

const source = await readFile(new URL("../../src/features/channels/channelMonitorViewModel.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    jsx: ts.JsxEmit.ReactJSX,
    module: ts.ModuleKind.ES2022,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText.replace('from "@/lib/time";', `from "${timeModuleUrl}";`);
const moduleUrl = `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`;
const viewModel = await import(moduleUrl);

const templates = [
  { id: "builtin-openai-chat-low-token", enabled: true, endpointKind: "chat_completions" },
  { id: "builtin-openai-responses-low-token", enabled: true, endpointKind: "responses" },
];

assert.equal(
  viewModel.preferredStationKeyMonitorTemplate(templates, {
    stationUpstreamApiFormat: "openai_responses",
    capabilities: { supportsChatCompletions: true, supportsResponses: true },
  })?.id,
  "builtin-openai-responses-low-token",
);

assert.equal(
  viewModel.preferredStationKeyMonitorTemplate(templates, {
    stationUpstreamApiFormat: "openai_chat_completions",
    capabilities: { supportsChatCompletions: true, supportsResponses: true },
  })?.id,
  "builtin-openai-chat-low-token",
);

assert.equal(
  viewModel.protocolForMonitorTemplate("builtin-openai-responses-low-token", templates),
  "responses",
);

assert.deepEqual(
  viewModel.monitorTemplateOptionsForProtocol(templates, "chat_completions").map((template) => template.id),
  ["builtin-openai-chat-low-token"],
);

console.log("channelMonitorViewModel tests passed");
