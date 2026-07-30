import type { ChannelMonitorClientProfileId, ChannelMonitorProtocolKind } from "@/lib/types/channelMonitors";

export const monitorProtocolCopy: Record<ChannelMonitorProtocolKind, { title: string; description: string }> = {
  open_ai_chat: {
    title: "OpenAI Chat 协议",
    description: "使用 /v1/chat/completions，适合标准 OpenAI 兼容渠道。",
  },
  open_ai_responses: {
    title: "OpenAI Responses 协议",
    description: "使用 /v1/responses，适合 Responses API 与 Codex 流量。",
  },
  anthropic_messages: {
    title: "Anthropic Messages 协议",
    description: "使用 /v1/messages，按 Anthropic 原生消息协议验证。",
  },
  gemini_native: {
    title: "Gemini 原生协议",
    description: "使用 Gemini generateContent 原生协议与响应语义。",
  },
  xai_grok: {
    title: "xAI / Grok 协议",
    description: "按 xAI Chat 协议执行受控低成本探测。",
  },
  generic_open_ai: {
    title: "通用 OpenAI 兼容协议",
    description: "用于无法严格归类但兼容 OpenAI Chat 请求的渠道。",
  },
};

export function protocolLabel(value: string) {
  return monitorProtocolCopy[value as ChannelMonitorProtocolKind]?.title ?? value;
}

export function profileLabel(id: string, cliCompat = id !== "standard_api") {
  const labels: Record<ChannelMonitorClientProfileId, string> = {
    standard_api: "标准 API 请求档案",
    codex_cli_compat: "Codex CLI 兼容档案",
    claude_code_compat: "Claude Code 兼容档案",
    gemini_cli_compat: "Gemini CLI 兼容档案",
    grok_cli_compat: "Grok CLI 兼容档案",
  };
  const label = labels[id as ChannelMonitorClientProfileId] ?? id;
  return cliCompat && !label.includes("兼容") ? `${label}（兼容模式）` : label;
}
