import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pageSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const stationKeysApiSource = await readFile("src/lib/api/stationKeys.ts", "utf8");
const commandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");

assert.ok(
  pageSource.includes("KeyConnectivityTestDialog"),
  "key pool page should render a dedicated connectivity test dialog",
);

assert.ok(
    pageSource.includes('data-testid="key-connectivity-test-dialog"') &&
    pageSource.includes('data-testid="key-connectivity-console-spinner"') &&
    pageSource.includes("选择测试模型") &&
    pageSource.includes("提示词：\"hi\""),
  "connectivity dialog should expose the visual test surface with model selection and an active testing spinner",
);

assert.ok(
  !/账号类型|选择账号类型|测试模式[\s\S]*<SelectControl|请求类型|常规请求|准备测试密钥|等待上游返回/.test(pageSource),
  "connectivity dialog should not expose account-type, test-mode, request-type, or redundant waiting rows",
);

assert.ok(
  pageSource.includes("displayedResponseText") &&
    !pageSource.includes("window.setInterval") &&
    pageSource.includes("responseTypingComplete") &&
    pageSource.includes("connectivityRunTokenRef"),
  "connectivity dialog should reveal real progress without owning a retained-page interval",
);

assert.ok(
  pageSource.includes('event.type === "attemptStarted"') &&
    pageSource.includes('event.type === "delta"') &&
    pageSource.includes('event.type === "fallback"') &&
    pageSource.includes("setDisplayedResponseText((current) => current + event.text)") &&
    pageSource.includes("setDisplayedResponseText(\"\")") &&
    pageSource.includes("setConnectivityStreamFallbackReason(event.reason)"),
  "connectivity dialog should consume typed stream progress, append deltas, and clear partial text before fallback",
);

assert.ok(
  pageSource.includes("border-border bg-surface-inset p-4 font-mono") &&
    !pageSource.includes("bg-slate-900") &&
    !pageSource.includes("border-slate-700"),
  "connectivity dialog transcript should use the semantic inset console surface",
);

const consoleBuilder = pageSource.match(/function buildConnectivityConsoleLines\([\s\S]*?function formatConnectivityDuration/)?.[0] ?? "";
assert.ok(
  consoleBuilder.includes("text-info-foreground") &&
    consoleBuilder.includes("text-success-foreground") &&
    consoleBuilder.includes("text-warning-foreground") &&
    consoleBuilder.includes("text-danger-foreground") &&
    !/text-(sky|cyan|emerald|amber|rose|slate)-/.test(consoleBuilder),
  "connectivity dialog transcript lines should use semantic status colors",
);

const resultBranch = pageSource.match(/if \(result\) \{[\s\S]*?if \(error\) \{/)?.[0] ?? "";
const responseModeIndex = resultBranch.indexOf("响应模式：");
const responseLabelIndex = resultBranch.indexOf("responseLabelLine");
assert.ok(
  pageSource.includes('const responseLabelLine = { text: "响应："') &&
    responseModeIndex >= 0 &&
    responseLabelIndex >= 0 &&
    responseModeIndex < responseLabelIndex,
  "completed connectivity results should show response mode before the response label",
);

assert.ok(
  pageSource.includes("buildKeyConnectivityModelOptions") &&
    pageSource.includes("connectivityCapabilities") &&
    pageSource.includes("getStationKeyCapabilities(item.id)") &&
    !pageSource.includes("Claude 3.5 Sonnet") &&
    !pageSource.includes("Gemini 2.5 Pro"),
  "connectivity dialog should derive test models from the key capability scope instead of showing unrelated static provider models",
);

assert.ok(
  stationKeysApiSource.includes('import { Channel, invoke } from "@tauri-apps/api/core"') &&
    stationKeysApiSource.includes("StationKeyConnectivityTestEvent") &&
    /testStationKeyConnectivity\(\s*stationKeyId: string,\s*model: string,\s*options: \{ onEvent\?: \(event: StationKeyConnectivityTestEvent\) => void \} = \{\},\s*\)/.test(stationKeysApiSource) &&
    stationKeysApiSource.includes("const progress = new Channel<StationKeyConnectivityTestEvent>()") &&
    stationKeysApiSource.includes("progress.onmessage = (event) => options.onEvent?.(event)") &&
    /invoke<StationKeyConnectivityTestResult>\("test_station_key_connectivity", \{ stationKeyId,\s*model,\s*progress \}\)/.test(stationKeysApiSource),
  "station key connectivity API should pass a request-scoped Tauri Channel to the selected-model probe",
);

assert.ok(
  /pub async fn test_station_key_connectivity\([\s\S]*station_key_id: String,[\s\S]*model: String,[\s\S]*progress: Channel<StationKeyConnectivityTestEvent>,[\s\S]*\)/.test(commandsSource) &&
    /test_station_key_connectivity_blocking\([\s\S]*&database,[\s\S]*&data_key,[\s\S]*&station_key_id,[\s\S]*&model,[\s\S]*progress,[\s\S]*\)/.test(commandsSource) &&
    /station_key_connectivity_model_candidates\(\s*capabilities\.as_ref\(\),\s*Some\(requested_model\.as_str\(\)\),\s*&discovered_models,\s*\)/.test(commandsSource),
  "Tauri connectivity command should accept a progress channel and prioritize the selected model for the probe",
);

assert.ok(
  commandsSource.includes("enum StationKeyConnectivityResponseMode") &&
    commandsSource.includes("response_mode: StationKeyConnectivityResponseMode") &&
    commandsSource.includes("stream_fallback_reason: Option<String>") &&
    commandsSource.includes("enum StationKeyConnectivityRequestMode") &&
    commandsSource.includes("StationKeyConnectivityRequestMode::Stream") &&
    commandsSource.includes("StationKeyConnectivityRequestMode::NonStream") &&
    commandsSource.includes('set("Accept", "text/event-stream")') &&
    commandsSource.includes('set("Accept", "application/json")'),
  "connectivity result and request path should expose stream mode metadata and switch Accept headers between stream and fallback",
);

assert.ok(
  commandsSource.includes("enum StationKeyConnectivityTestEvent") &&
    commandsSource.includes("AttemptStarted") &&
    commandsSource.includes("Delta") &&
    commandsSource.includes("Fallback") &&
    commandsSource.includes("StationKeyConnectivitySseDecoder") &&
    commandsSource.includes("response.output_text.delta") &&
    commandsSource.includes("[DONE]"),
  "backend should emit typed progress events and parse real Responses/Chat SSE streams",
);

assert.ok(
  commandsSource.includes('"input": "hi"') &&
    commandsSource.includes('"content": "hi"') &&
    commandsSource.includes("extract_station_key_connectivity_reply"),
  "connectivity probe should send the same hi prompt shown in the dialog and extract the real upstream reply",
);

console.log("key pool connectivity dialog contract passed");
