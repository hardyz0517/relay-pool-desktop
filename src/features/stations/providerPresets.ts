import type { StationType } from "@/lib/types/stations";

export type ProviderPresetId =
  | "custom"
  | "kamiapi"
  | "deepseek"
  | "qwen"
  | "zhipu"
  | "kimi"
  | "doubao"
  | "hunyuan"
  | "qianfan"
  | "siliconflow"
  | "minimax"
  | "stepfun"
  | "mimo"
  | "lingyiwanwu"
  | "baichuan";

export type ProviderPreset = {
  id: ProviderPresetId;
  name: string;
  description: string;
  stationType: StationType;
  websiteUrl: string;
  apiBaseUrl: string;
};

export const providerPresets: ProviderPreset[] = [
  {
    id: "custom",
    name: "自定义配置",
    description: "手动填写供应商名称、接口地址和站点类型。",
    stationType: "custom",
    websiteUrl: "",
    apiBaseUrl: "",
  },
  {
    id: "kamiapi",
    name: "卡米API",
    description: "NewAPI 魔改站，推荐使用网页登录授权完成会话采集。",
    stationType: "newapi",
    websiteUrl: "https://www.kamiapi.top",
    apiBaseUrl: "https://www.kamiapi.top/v1",
  },
  {
    id: "deepseek",
    name: "DeepSeek",
    description: "DeepSeek 官方兼容入口。",
    stationType: "custom",
    websiteUrl: "https://platform.deepseek.com",
    apiBaseUrl: "https://api.deepseek.com/v1",
  },
  {
    id: "qwen",
    name: "Qwen",
    description: "通义千问兼容入口。",
    stationType: "custom",
    websiteUrl: "https://bailian.console.aliyun.com",
    apiBaseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
  },
  {
    id: "zhipu",
    name: "智谱 GLM",
    description: "智谱 AI GLM 官方兼容入口。",
    stationType: "custom",
    websiteUrl: "https://open.bigmodel.cn",
    apiBaseUrl: "https://open.bigmodel.cn/api/paas/v4",
  },
  {
    id: "kimi",
    name: "Kimi",
    description: "Moonshot AI Kimi 官方兼容入口。",
    stationType: "custom",
    websiteUrl: "https://platform.moonshot.cn",
    apiBaseUrl: "https://api.moonshot.ai/v1",
  },
  {
    id: "doubao",
    name: "豆包",
    description: "火山方舟豆包兼容入口。",
    stationType: "custom",
    websiteUrl: "https://console.volcengine.com/ark",
    apiBaseUrl: "https://ark.cn-beijing.volces.com/api/v3",
  },
  {
    id: "hunyuan",
    name: "腾讯混元",
    description: "腾讯混元官方兼容入口。",
    stationType: "custom",
    websiteUrl: "https://cloud.tencent.com/product/hunyuan",
    apiBaseUrl: "https://api.hunyuan.cloud.tencent.com/v1",
  },
  {
    id: "qianfan",
    name: "百度千帆",
    description: "百度智能云千帆兼容入口。",
    stationType: "custom",
    websiteUrl: "https://console.bce.baidu.com/qianfan/ais/console",
    apiBaseUrl: "https://qianfan.baidubce.com/v2",
  },
  {
    id: "siliconflow",
    name: "SiliconFlow",
    description: "硅基流动兼容入口。",
    stationType: "custom",
    websiteUrl: "https://cloud.siliconflow.cn",
    apiBaseUrl: "https://api.siliconflow.cn/v1",
  },
  {
    id: "minimax",
    name: "MiniMax",
    description: "MiniMax 兼容入口。",
    stationType: "custom",
    websiteUrl: "https://platform.minimaxi.com",
    apiBaseUrl: "https://api.minimax.io/v1",
  },
  {
    id: "stepfun",
    name: "阶跃星辰",
    description: "StepFun 阶跃星辰兼容入口。",
    stationType: "custom",
    websiteUrl: "https://platform.stepfun.com",
    apiBaseUrl: "https://api.stepfun.com/v1",
  },
  {
    id: "mimo",
    name: "小米 MiMo",
    description: "小米 MiMo 官方兼容入口。",
    stationType: "custom",
    websiteUrl: "https://platform.xiaomimimo.com",
    apiBaseUrl: "https://api.xiaomimimo.com/v1",
  },
  {
    id: "lingyiwanwu",
    name: "零一万物",
    description: "零一万物 Yi 兼容入口。",
    stationType: "custom",
    websiteUrl: "https://platform.lingyiwanwu.com",
    apiBaseUrl: "https://api.lingyiwanwu.com/v1",
  },
  {
    id: "baichuan",
    name: "百川智能",
    description: "百川智能官方兼容入口。",
    stationType: "custom",
    websiteUrl: "https://platform.baichuan-ai.com",
    apiBaseUrl: "https://api.baichuan-ai.com/v1",
  },
];
