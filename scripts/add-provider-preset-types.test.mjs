import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const presetsSource = await readFile("src/features/stations/providerPresets.ts", "utf8");
const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
const stationTypesSource = await readFile("src/lib/types/stations.ts", "utf8");

assert.ok(
  presetsSource.includes('name: "自定义配置"'),
  "supplier presets should include a clear custom configuration option",
);

assert.ok(
  !presetsSource.includes('id: "sub2api"') && !presetsSource.includes('id: "newapi"'),
  "Sub2API and NewAPI should not appear as supplier presets",
);

assert.ok(
  presetsSource.includes('stationType: "custom"'),
  "generic supplier presets should save as the merged custom station type",
);

[
  ['id: "zhipu"', 'name: "智谱 GLM"', 'baseUrl: "https://open.bigmodel.cn/api/paas/v4"'],
  ['id: "kimi"', 'name: "Kimi"', 'baseUrl: "https://api.moonshot.ai/v1"'],
  ['id: "doubao"', 'name: "豆包"', 'baseUrl: "https://ark.cn-beijing.volces.com/api/v3"'],
  ['id: "hunyuan"', 'name: "腾讯混元"', 'baseUrl: "https://api.hunyuan.cloud.tencent.com/v1"'],
  ['id: "qianfan"', 'name: "百度千帆"', 'baseUrl: "https://qianfan.baidubce.com/v2"'],
  ['id: "stepfun"', 'name: "阶跃星辰"', 'baseUrl: "https://api.stepfun.com/v1"'],
  ['id: "mimo"', 'name: "小米 MiMo"', 'baseUrl: "https://api.xiaomimimo.com/v1"'],
  ['id: "lingyiwanwu"', 'name: "零一万物"', 'baseUrl: "https://api.lingyiwanwu.com/v1"'],
  ['id: "baichuan"', 'name: "百川智能"', 'baseUrl: "https://api.baichuan-ai.com/v1"'],
].forEach(([idSnippet, nameSnippet, baseUrlSnippet]) => {
  assert.ok(presetsSource.includes(idSnippet), `supplier presets should include ${idSnippet}`);
  assert.ok(presetsSource.includes(nameSnippet), `supplier presets should label ${nameSnippet}`);
  assert.ok(presetsSource.includes(baseUrlSnippet), `supplier presets should use ${baseUrlSnippet}`);
});

assert.ok(
  presetsSource.includes('baseUrl: "https://api.minimax.io/v1"'),
  "MiniMax preset should use the current OpenAI-compatible official API host",
);

assert.ok(
  !addProviderSource.includes("const defaultPreset = providerPresets[1]"),
  "new supplier forms should default to the custom configuration preset",
);

assert.ok(
  addProviderSource.includes("function getPresetDefaultStationName"),
  "new supplier forms should centralize preset-to-station-name defaults",
);

assert.ok(
  addProviderSource.includes('preset.id === "custom" ? "" : preset.name'),
  "custom configuration should not prefill the supplier name field",
);

assert.ok(
  !addProviderSource.includes("name: defaultPreset.name"),
  "custom default form should start with an empty supplier name",
);

assert.ok(
  stationTypesSource.includes('"openai-compatible": "自定义接口"'),
  "legacy OpenAI-compatible station type should display as the merged custom interface type",
);

assert.ok(
  stationTypesSource.includes('custom: "自定义接口"'),
  "custom station type should display as the merged custom interface type",
);
