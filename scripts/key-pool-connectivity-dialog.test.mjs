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
    pageSource.includes("window.setInterval") &&
    pageSource.includes("responseTypingComplete"),
  "connectivity dialog should reveal the final response with a typewriter effect",
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
  /testStationKeyConnectivity\(stationKeyId: string,\s*model: string\)/.test(stationKeysApiSource) &&
    /invoke<StationKeyConnectivityTestResult>\("test_station_key_connectivity", \{ stationKeyId,\s*model \}\)/.test(stationKeysApiSource),
  "station key connectivity API should pass the selected model to Tauri",
);

assert.ok(
  /pub async fn test_station_key_connectivity\([\s\S]*station_key_id: String,[\s\S]*model: String,[\s\S]*\)/.test(commandsSource) &&
    /test_station_key_connectivity_blocking\(&database, &data_key, &station_key_id, &model\)/.test(commandsSource) &&
    /station_key_connectivity_model_candidates\(\s*capabilities\.as_ref\(\),\s*Some\(requested_model\.as_str\(\)\),\s*&discovered_models,\s*\)/.test(commandsSource),
  "Tauri connectivity command should accept the selected model and prioritize it for the probe",
);

assert.ok(
  commandsSource.includes('"input": "hi"') &&
    commandsSource.includes('"content": "hi"') &&
    commandsSource.includes("extract_station_key_connectivity_reply"),
  "connectivity probe should send the same hi prompt shown in the dialog and extract the real upstream reply",
);

console.log("key pool connectivity dialog contract passed");
