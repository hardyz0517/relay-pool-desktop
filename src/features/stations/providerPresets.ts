import type { StationType } from "@/lib/types/stations";

export type ProviderPresetId =
  | "custom"
  | "openai-compatible"
  | "sub2api"
  | "newapi"
  | "deepseek"
  | "qwen"
  | "siliconflow"
  | "minimax";

export type ProviderPreset = {
  id: ProviderPresetId;
  name: string;
  description: string;
  stationType: StationType;
  baseUrl: string;
};

export const providerPresets: ProviderPreset[] = [
  {
    id: "custom",
    name: "自定义",
    description: "完全自定义供应商。",
    stationType: "custom",
    baseUrl: "",
  },
  {
    id: "openai-compatible",
    name: "兼容 OpenAI",
    description: "适用于大多数兼容 /v1 的中转站。",
    stationType: "openai-compatible",
    baseUrl: "https://api.example.com/v1",
  },
  {
    id: "sub2api",
    name: "Sub2API",
    description: "带订阅与采集能力的 Sub2API 站点。",
    stationType: "sub2api",
    baseUrl: "https://sub2api.example.com/v1",
  },
  {
    id: "newapi",
    name: "NewAPI",
    description: "NewAPI 风格站点。",
    stationType: "newapi",
    baseUrl: "https://newapi.example.com/v1",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    description: "DeepSeek 官方兼容入口。",
    stationType: "openai-compatible",
    baseUrl: "https://api.deepseek.com/v1",
  },
  {
    id: "qwen",
    name: "Qwen",
    description: "通义千问兼容入口。",
    stationType: "openai-compatible",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
  },
  {
    id: "siliconflow",
    name: "SiliconFlow",
    description: "硅基流动兼容入口。",
    stationType: "openai-compatible",
    baseUrl: "https://api.siliconflow.cn/v1",
  },
  {
    id: "minimax",
    name: "MiniMax",
    description: "MiniMax 兼容入口。",
    stationType: "openai-compatible",
    baseUrl: "https://api.minimax.chat/v1",
  },
];
