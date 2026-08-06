import { describe, expect, it } from "vitest";

import {
  createEmptyMonitorDraft,
  createStationKeyMonitorInput,
  draftToMonitorInput,
  monitorToDraft,
  selectStationKeyMonitorModel,
  validateMonitorDraft,
} from "@/lib/channelMonitorViewModel";
import type {
  ChannelMonitor,
  ChannelMonitorRequestTemplate,
  MonitoringCapabilityCatalog,
} from "@/lib/types/channelMonitors";

const templates: ChannelMonitorRequestTemplate[] = [
  {
    id: "builtin-openai-chat-low-token",
    name: "OpenAI Chat low token",
    endpointKind: "chat_completions",
    method: "POST",
    path: "/v1/chat/completions",
    requestBodyJson: "{}",
    enabled: true,
    builtIn: true,
    note: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  },
];

const capabilities: MonitoringCapabilityCatalog = {
  protocols: [
    { id: "open_ai_chat", enabled: true, streaming: false },
    { id: "anthropic_messages", enabled: true, streaming: false },
    { id: "xai_grok", enabled: false, streaming: false },
  ],
  profiles: [
    {
      id: "standard_api",
      version: 1,
      enabled: true,
      cliCompat: false,
      supportedProtocols: ["open_ai_chat", "anthropic_messages"],
      method: "POST",
      path: "{adapter_path}",
      headerNames: ["content-type"],
      bodyDefaults: ["max_tokens"],
      profileHash: "standard-v1",
    },
    {
      id: "claude_code_compat",
      version: 2,
      enabled: true,
      cliCompat: true,
      supportedProtocols: ["anthropic_messages"],
      method: "POST",
      path: "/v1/messages",
      headerNames: ["anthropic-version"],
      bodyDefaults: ["max_tokens"],
      profileHash: "claude-v2",
    },
  ],
};

function validDraft() {
  return {
    ...createEmptyMonitorDraft([], templates, capabilities),
    name: "Primary monitor",
    targetType: "station" as const,
    stationId: "station-1",
    stationKeyId: "",
    primaryModel: "gpt-4.1-mini",
  };
}

describe("channel monitor V2 view model", () => {
  it("validates protocol and Profile capabilities together", () => {
    const draft = validDraft();

    expect(validateMonitorDraft({
      ...draft,
      protocolKind: "anthropic_messages",
      clientProfileId: "claude_code_compat",
      clientProfileVersion: "2",
    }, { templates, keys: [], capabilities })).toBeNull();

    expect(validateMonitorDraft({
      ...draft,
      protocolKind: "open_ai_chat",
      clientProfileId: "claude_code_compat",
      clientProfileVersion: "2",
    }, { templates, keys: [], capabilities })).toBe("请求 Profile 不支持当前协议");

    expect(validateMonitorDraft({
      ...draft,
      protocolKind: "xai_grok",
    }, { templates, keys: [], capabilities })).toBe("请选择可用的请求协议");
  });

  it("normalizes primary and ordered fallback models into the V2 input", () => {
    const input = draftToMonitorInput({
      ...validDraft(),
      primaryModel: "  gpt-4.1-mini  ",
      fallbackModels: [" claude-3-5-haiku ", "", " gemini-2.0-flash "],
      retryMaxAttemptsPerModel: "2",
      healthFailureThreshold: "4",
      executionTimeoutMs: "30500",
    });

    expect(input.primaryModel).toBe("gpt-4.1-mini");
    expect(input.fallbackModels).toEqual(["claude-3-5-haiku", "gemini-2.0-flash"]);
    expect(input.retryMaxAttemptsPerModel).toBe(2);
    expect(input.consecutiveFailureThreshold).toBe(4);
    expect(input.timeoutSeconds).toBe(31);
  });

  it("hydrates every editable V2 field from an existing monitor", () => {
    const draft = monitorToDraft({
      name: "Existing monitor",
      targetType: "station_key",
      stationId: "station-1",
      stationKeyId: "key-1",
      templateId: "builtin-openai-chat-low-token",
      enabled: false,
      protocolKind: "anthropic_messages",
      clientProfileId: "claude_code_compat",
      clientProfileVersion: 2,
      primaryModel: "claude-3-5-sonnet",
      fallbackModels: ["claude-3-5-haiku"],
      intervalSeconds: 180,
      jitterSeconds: 12,
      attemptTimeoutMs: 12_000,
      executionTimeoutMs: 45_000,
      retryMaxAttemptsPerModel: 2,
      retryInitialBackoffMs: 500,
      retryMaxBackoffMs: 4_000,
      riskDailyProbeBudget: 80,
      healthPolicyMode: "observe_only",
      healthFailureThreshold: 4,
      healthRecoveryThreshold: 3,
      note: "existing note",
    } as ChannelMonitor);

    expect(draft).toMatchObject({
      name: "Existing monitor",
      enabled: false,
      protocolKind: "anthropic_messages",
      clientProfileId: "claude_code_compat",
      clientProfileVersion: "2",
      primaryModel: "claude-3-5-sonnet",
      fallbackModels: ["claude-3-5-haiku"],
      intervalSeconds: "180",
      jitterSeconds: "12",
      attemptTimeoutMs: "12000",
      executionTimeoutMs: "45000",
      retryMaxAttemptsPerModel: "2",
      retryInitialBackoffMs: "500",
      retryMaxBackoffMs: "4000",
      riskDailyProbeBudget: "80",
      healthPolicyMode: "observe_only",
      healthFailureThreshold: "4",
      healthRecoveryThreshold: "3",
      note: "existing note",
    });
  });

  it("blocks authoritative health writeback for compatibility Profiles", () => {
    expect(validateMonitorDraft({
      ...validDraft(),
      protocolKind: "anthropic_messages",
      clientProfileId: "claude_code_compat",
      clientProfileVersion: "2",
      healthPolicyMode: "authoritative",
    }, { templates, keys: [], capabilities })).toBe("权威健康写回只能使用标准 API Profile");
  });

  it("creates Key Pool monitors with the complete conservative V2 defaults", () => {
    const input = createStationKeyMonitorInput(
      { id: "key-1", stationId: "station-1", name: "Key A" },
      { id: "builtin-openai-chat-low-token", endpointKind: "chat_completions" },
      {
        preferredModels: ["gpt-4o-mini"],
        modelAllowlist: ["gpt-5.5"],
        modelBlocklist: [],
      },
      "gpt",
    );

    expect(input).toMatchObject({
      protocolKind: "open_ai_chat",
      clientProfileId: "standard_api",
      clientProfileVersion: 1,
      primaryModel: "gpt-5.5",
      retryMaxAttemptsPerModel: 1,
      retryInitialBackoffMs: 200,
      retryMaxBackoffMs: 2_000,
      riskDailyProbeBudget: 200,
      healthPolicyMode: "observe_only",
      healthFailureThreshold: 2,
      healthRecoveryThreshold: 2,
      attemptTimeoutMs: 45_000,
      executionTimeoutMs: 60_000,
      intervalSeconds: 300,
      jitterSeconds: 15,
      fallbackModels: [],
    });
  });

  it.each([
    ["gpt", "gpt-5.5"],
    ["claude", "claude-opus-4-8"],
    ["gemini", "gemini-3.5-flash"],
    ["grok", "grok-4.5"],
  ] as const)("uses the %s group default model for Key Pool monitoring", (category, model) => {
    expect(selectStationKeyMonitorModel(null, category)).toBe(model);
  });

  it("keeps explicit Key model restrictions ahead of the group default", () => {
    expect(selectStationKeyMonitorModel({
      preferredModels: ["claude-sonnet-4-7"],
      modelAllowlist: ["claude-sonnet-4-7"],
      modelBlocklist: ["claude-opus-4-8"],
    }, "claude")).toBe("claude-sonnet-4-7");
  });
});
