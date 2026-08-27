import type { StationGroupCategory } from "@/lib/groupCategories";
import type {
  ChannelMonitor,
  ChannelMonitorClientProfileId,
  ChannelMonitorHealthWritebackMode,
  ChannelMonitorProtocolKind,
  ChannelMonitorRequestTemplate,
  ChannelMonitorTargetType,
  CreateChannelMonitorInput,
  MonitoringCapabilityCatalog,
} from "@/lib/types/channelMonitors";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import { toTimestampMillis } from "@/lib/time";

export type ChannelMonitorDraft = {
  name: string;
  targetType: ChannelMonitorTargetType;
  stationId: string;
  stationKeyId: string;
  templateId: string;
  enabled: boolean;
  pauseOnZeroBalance: boolean;
  proxyMode: "inherit" | "direct" | "system" | "manual";
  proxyUrl: string;
  protocolKind: ChannelMonitorProtocolKind;
  clientProfileId: ChannelMonitorClientProfileId;
  clientProfileVersion: string;
  intervalSeconds: string;
  jitterSeconds: string;
  primaryModel: string;
  fallbackModels: string[];
  attemptTimeoutMs: string;
  executionTimeoutMs: string;
  retryMaxAttemptsPerModel: string;
  retryInitialBackoffMs: string;
  retryMaxBackoffMs: string;
  riskDailyProbeBudget: string;
  healthPolicyMode: ChannelMonitorHealthWritebackMode;
  healthFailureThreshold: string;
  healthRecoveryThreshold: string;
  note: string;
};

type MonitorValidationContext = {
  templates: ChannelMonitorRequestTemplate[];
  keys: KeyPoolItem[];
  capabilities: MonitoringCapabilityCatalog | undefined;
};

type StationKeyMonitorTemplatePreference = {
  stationType?: string | null;
  stationUpstreamApiFormat?: string | null;
  capabilities?: Pick<StationKeyCapabilities, "supportsChatCompletions" | "supportsResponses"> | null;
};

export const DEFAULT_STATION_KEY_MONITOR_MODEL = "gpt-4.1-mini";
export const DEFAULT_STATION_KEY_MONITOR_MODELS: Partial<Record<StationGroupCategory, string>> = {
  gpt: "gpt-5.5",
  claude: "claude-opus-4-8",
  gemini: "gemini-3.5-flash",
  grok: "grok-4.5",
};
export const DEFAULT_STATION_KEY_MONITOR_TEMPLATE_ID = "builtin-openai-responses-low-token";
export const STATION_KEY_MONITOR_NOTE = "由密钥池监控开关创建";
const DEFAULT_MONITOR_ATTEMPT_TIMEOUT_MS = 45_000;
const DEFAULT_MONITOR_EXECUTION_TIMEOUT_MS = 60_000;

export const targetTypeOptions: Array<{ value: ChannelMonitorTargetType; label: string }> = [
  { value: "station_key", label: "单个密钥" },
  { value: "station", label: "中转站全部启用密钥" },
];

export function findStationKeyMonitor(
  monitors: ChannelMonitor[],
  stationKeyId: string,
) {
  return monitors
    .filter((monitor) => monitor.targetType === "station_key" && monitor.stationKeyId === stationKeyId)
    .sort((a, b) => toTime(b.updatedAt) - toTime(a.updatedAt))[0] ?? null;
}

export function preferredStationKeyMonitorTemplate(
  templates: Array<Pick<ChannelMonitorRequestTemplate, "id" | "enabled" | "endpointKind">>,
  preference: StationKeyMonitorTemplatePreference = {},
) {
  const chatTemplate = templates.find((template) => template.enabled && template.id === "builtin-openai-chat-low-token") ??
    templates.find((template) => template.enabled && template.endpointKind === "chat_completions") ??
    null;
  const responsesTemplate = templates.find((template) => template.enabled && template.id === DEFAULT_STATION_KEY_MONITOR_TEMPLATE_ID) ??
    templates.find((template) => template.enabled && template.endpointKind === "responses") ??
    null;
  const supportsChat = preference.capabilities?.supportsChatCompletions !== false;
  const supportsResponses = preference.capabilities?.supportsResponses !== false;

  if (preference.stationUpstreamApiFormat === "openai_chat_completions" && supportsChat) {
    return chatTemplate ?? responsesTemplate ?? templates.find((template) => template.enabled) ?? null;
  }
  if (preference.stationUpstreamApiFormat === "openai_responses" && supportsResponses) {
    return responsesTemplate ?? chatTemplate ?? templates.find((template) => template.enabled) ?? null;
  }
  if (!supportsResponses && supportsChat) {
    return chatTemplate ?? templates.find((template) => template.enabled) ?? null;
  }
  if (!supportsChat && supportsResponses) {
    return responsesTemplate ?? templates.find((template) => template.enabled) ?? null;
  }

  return responsesTemplate ??
    chatTemplate ??
    templates.find((template) => template.enabled) ??
    null;
}

export function templateForMonitorProtocol(
  templates: ChannelMonitorRequestTemplate[],
  protocol: ChannelMonitorProtocolKind,
) {
  const endpointKind = protocol === "open_ai_responses" ? "responses" : "chat_completions";
  return templates.find((template) => template.enabled && template.endpointKind === endpointKind)
    ?? templates.find((template) => template.enabled)
    ?? null;
}

export function selectStationKeyMonitorModel(
  capabilities?: Pick<StationKeyCapabilities, "modelAllowlist" | "modelBlocklist" | "preferredModels"> | null,
  groupCategory?: StationGroupCategory | null,
) {
  const blockedModels = new Set((capabilities?.modelBlocklist ?? []).map(normalizeModelName));
  const allowlistedModels = new Set((capabilities?.modelAllowlist ?? []).map(normalizeModelName));
  const explicitModels = [
    ...(capabilities?.preferredModels ?? []),
    ...(capabilities?.modelAllowlist ?? []),
  ];
  const candidates = uniqueModels(explicitModels).filter((model) => !blockedModels.has(normalizeModelName(model)));
  const groupDefault = groupCategory ? DEFAULT_STATION_KEY_MONITOR_MODELS[groupCategory] : undefined;
  if (
    groupDefault &&
    !blockedModels.has(normalizeModelName(groupDefault)) &&
    (allowlistedModels.size === 0 || allowlistedModels.has(normalizeModelName(groupDefault)))
  ) {
    return groupDefault;
  }
  const selected = candidates[0];
  return selected ?? (blockedModels.has(normalizeModelName(DEFAULT_STATION_KEY_MONITOR_MODEL))
    ? candidates[0] ?? DEFAULT_STATION_KEY_MONITOR_MODEL
    : DEFAULT_STATION_KEY_MONITOR_MODEL);
}

export function createStationKeyMonitorInput(
  key: Pick<KeyPoolItem, "id" | "stationId" | "name">,
  template: Pick<ChannelMonitorRequestTemplate, "id" | "endpointKind">,
  capabilities?: Pick<StationKeyCapabilities, "modelAllowlist" | "modelBlocklist" | "preferredModels"> | null,
  groupCategory?: StationGroupCategory | null,
): CreateChannelMonitorInput {
  const fallbackModel = selectStationKeyMonitorModel(capabilities, groupCategory);
  return {
    name: `${key.name} 监控`,
    targetType: "station_key",
    stationId: key.stationId,
    stationKeyId: key.id,
    templateId: template.id,
    enabled: true,
    pauseOnZeroBalance: true,
    proxyMode: "inherit",
    proxyUrl: "",
    protocolKind: template.endpointKind === "responses" ? "open_ai_responses" : "open_ai_chat",
    clientProfileId: "standard_api",
    clientProfileVersion: 1,
    primaryModel: fallbackModel,
    retryMaxAttemptsPerModel: 1,
    retryInitialBackoffMs: 200,
    retryMaxBackoffMs: 2_000,
    riskDailyProbeBudget: 2_000,
    healthPolicyMode: "observe_only",
    healthFailureThreshold: 2,
    healthRecoveryThreshold: 2,
    attemptTimeoutMs: DEFAULT_MONITOR_ATTEMPT_TIMEOUT_MS,
    executionTimeoutMs: DEFAULT_MONITOR_EXECUTION_TIMEOUT_MS,
    intervalSeconds: 300,
    jitterSeconds: 15,
    timeoutSeconds: 30,
    maxConcurrency: 1,
    consecutiveFailureThreshold: 3,
    fallbackModels: [],
    note: STATION_KEY_MONITOR_NOTE,
  };
}

export function updateStationKeyMonitorEnabledInput(
  monitor: ChannelMonitor,
  enabled: boolean,
) {
  return {
    id: monitor.id,
    name: monitor.name,
    targetType: monitor.targetType,
    stationId: monitor.stationId,
    stationKeyId: monitor.stationKeyId,
    templateId: monitor.templateId,
    enabled,
    pauseOnZeroBalance: monitor.pauseOnZeroBalance,
    proxyMode: monitor.proxyMode,
    proxyUrl: monitor.proxyUrl ?? "",
    protocolKind: monitor.protocolKind,
    clientProfileId: monitor.clientProfileId,
    clientProfileVersion: monitor.clientProfileVersion,
    primaryModel: monitor.primaryModel,
    retryMaxAttemptsPerModel: monitor.retryMaxAttemptsPerModel,
    retryInitialBackoffMs: monitor.retryInitialBackoffMs,
    retryMaxBackoffMs: monitor.retryMaxBackoffMs,
    riskDailyProbeBudget: monitor.riskDailyProbeBudget,
    healthPolicyMode: monitor.healthPolicyMode,
    healthFailureThreshold: monitor.healthFailureThreshold,
    healthRecoveryThreshold: monitor.healthRecoveryThreshold,
    attemptTimeoutMs: monitor.attemptTimeoutMs,
    executionTimeoutMs: monitor.executionTimeoutMs,
    intervalSeconds: monitor.intervalSeconds,
    jitterSeconds: monitor.jitterSeconds,
    timeoutSeconds: monitor.timeoutSeconds,
    maxConcurrency: monitor.maxConcurrency,
    consecutiveFailureThreshold: monitor.consecutiveFailureThreshold,
    fallbackModels: [...monitor.fallbackModels],
    note: monitor.note,
  };
}

export function createEmptyMonitorDraft(
  stations: Station[] = [],
  templates: ChannelMonitorRequestTemplate[] = [],
  capabilities?: MonitoringCapabilityCatalog,
): ChannelMonitorDraft {
  const stationId = stations[0]?.id ?? "";
  const firstTemplate = templates.find((template) => template.enabled);
  const protocolKind: ChannelMonitorProtocolKind = firstTemplate?.endpointKind === "responses"
    ? "open_ai_responses"
    : "open_ai_chat";
  const standardProfile = capabilities?.profiles.find((profile) => profile.id === "standard_api");
  return {
    name: "",
    targetType: "station_key",
    stationId,
    stationKeyId: "",
    templateId: firstTemplate?.id ?? "",
    enabled: true,
    pauseOnZeroBalance: true,
    proxyMode: "inherit",
    proxyUrl: "",
    protocolKind,
    clientProfileId: "standard_api",
    clientProfileVersion: String(standardProfile?.version ?? 1),
    intervalSeconds: "300",
    jitterSeconds: "30",
    primaryModel: "",
    fallbackModels: [],
    attemptTimeoutMs: String(DEFAULT_MONITOR_ATTEMPT_TIMEOUT_MS),
    executionTimeoutMs: String(DEFAULT_MONITOR_EXECUTION_TIMEOUT_MS),
    retryMaxAttemptsPerModel: "1",
    retryInitialBackoffMs: "200",
    retryMaxBackoffMs: "2000",
    riskDailyProbeBudget: "200",
    healthPolicyMode: "observe_only",
    healthFailureThreshold: "2",
    healthRecoveryThreshold: "2",
    note: "",
  };
}

export function monitorToDraft(monitor: ChannelMonitor): ChannelMonitorDraft {
  return {
    name: monitor.name,
    targetType: monitor.targetType,
    stationId: monitor.stationId,
    stationKeyId: monitor.stationKeyId ?? "",
    templateId: monitor.templateId,
    enabled: monitor.enabled,
    pauseOnZeroBalance: monitor.pauseOnZeroBalance,
    proxyMode: monitor.proxyMode,
    proxyUrl: monitor.proxyUrl ?? "",
    protocolKind: monitor.protocolKind,
    clientProfileId: monitor.clientProfileId,
    clientProfileVersion: String(monitor.clientProfileVersion),
    intervalSeconds: String(monitor.intervalSeconds),
    jitterSeconds: String(monitor.jitterSeconds),
    primaryModel: monitor.primaryModel,
    fallbackModels: [...monitor.fallbackModels],
    attemptTimeoutMs: String(monitor.attemptTimeoutMs),
    executionTimeoutMs: String(monitor.executionTimeoutMs),
    retryMaxAttemptsPerModel: String(monitor.retryMaxAttemptsPerModel),
    retryInitialBackoffMs: String(monitor.retryInitialBackoffMs),
    retryMaxBackoffMs: String(monitor.retryMaxBackoffMs),
    riskDailyProbeBudget: String(monitor.riskDailyProbeBudget),
    healthPolicyMode: monitor.healthPolicyMode,
    healthFailureThreshold: String(monitor.healthFailureThreshold),
    healthRecoveryThreshold: String(monitor.healthRecoveryThreshold),
    note: monitor.note ?? "",
  };
}

export function monitorToCreateInput(monitor: ChannelMonitor, name = `${monitor.name} 副本`): CreateChannelMonitorInput {
  return {
    name,
    targetType: monitor.targetType,
    stationId: monitor.stationId,
    stationKeyId: monitor.targetType === "station_key" ? monitor.stationKeyId : null,
    templateId: monitor.templateId,
    enabled: monitor.enabled,
    pauseOnZeroBalance: monitor.pauseOnZeroBalance,
    proxyMode: monitor.proxyMode,
    proxyUrl: monitor.proxyUrl,
    protocolKind: monitor.protocolKind,
    clientProfileId: monitor.clientProfileId,
    clientProfileVersion: monitor.clientProfileVersion,
    primaryModel: monitor.primaryModel,
    retryMaxAttemptsPerModel: monitor.retryMaxAttemptsPerModel,
    retryInitialBackoffMs: monitor.retryInitialBackoffMs,
    retryMaxBackoffMs: monitor.retryMaxBackoffMs,
    riskDailyProbeBudget: monitor.riskDailyProbeBudget,
    healthPolicyMode: monitor.healthPolicyMode,
    healthFailureThreshold: monitor.healthFailureThreshold,
    healthRecoveryThreshold: monitor.healthRecoveryThreshold,
    attemptTimeoutMs: monitor.attemptTimeoutMs,
    executionTimeoutMs: monitor.executionTimeoutMs,
    intervalSeconds: monitor.intervalSeconds,
    jitterSeconds: monitor.jitterSeconds,
    timeoutSeconds: monitor.timeoutSeconds,
    maxConcurrency: monitor.maxConcurrency,
    consecutiveFailureThreshold: monitor.consecutiveFailureThreshold,
    fallbackModels: [...monitor.fallbackModels],
    note: monitor.note,
  };
}

export function draftToMonitorInput(draft: ChannelMonitorDraft): CreateChannelMonitorInput {
  const primaryModel = draft.primaryModel.trim();
  const fallbackModels = draft.fallbackModels
    .map((model) => model.trim())
    .filter((model) => model && model !== primaryModel);
  const executionTimeoutMs = toInteger(draft.executionTimeoutMs);
  return {
    name: draft.name.trim(),
    targetType: draft.targetType,
    stationId: draft.stationId,
    stationKeyId: draft.targetType === "station_key" ? draft.stationKeyId : null,
    templateId: draft.templateId,
    enabled: draft.enabled,
    pauseOnZeroBalance: draft.pauseOnZeroBalance,
    proxyMode: draft.proxyMode ?? "inherit",
    proxyUrl: (draft.proxyMode ?? "inherit") === "manual" && (draft.proxyUrl ?? "").trim() ? (draft.proxyUrl ?? "").trim() : null,
    protocolKind: draft.protocolKind,
    clientProfileId: draft.clientProfileId,
    clientProfileVersion: toInteger(draft.clientProfileVersion),
    primaryModel,
    retryMaxAttemptsPerModel: toInteger(draft.retryMaxAttemptsPerModel),
    retryInitialBackoffMs: toInteger(draft.retryInitialBackoffMs),
    retryMaxBackoffMs: toInteger(draft.retryMaxBackoffMs),
    riskDailyProbeBudget: toInteger(draft.riskDailyProbeBudget),
    healthPolicyMode: draft.healthPolicyMode,
    healthFailureThreshold: toInteger(draft.healthFailureThreshold),
    healthRecoveryThreshold: toInteger(draft.healthRecoveryThreshold),
    attemptTimeoutMs: toInteger(draft.attemptTimeoutMs),
    executionTimeoutMs,
    intervalSeconds: toInteger(draft.intervalSeconds),
    jitterSeconds: toInteger(draft.jitterSeconds),
    timeoutSeconds: Math.max(5, Math.min(120, Math.ceil(executionTimeoutMs / 1_000))),
    maxConcurrency: draft.targetType === "station" ? 2 : 1,
    consecutiveFailureThreshold: toInteger(draft.healthFailureThreshold),
    fallbackModels,
    note: draft.note.trim() ? draft.note.trim() : null,
  };
}

export function validateMonitorDraft(
  draft: ChannelMonitorDraft,
  { templates, keys, capabilities }: MonitorValidationContext,
): string | null {
  const intervalSeconds = parseInteger(draft.intervalSeconds);
  const jitterSeconds = parseInteger(draft.jitterSeconds);
  const clientProfileVersion = parseInteger(draft.clientProfileVersion);
  const retryMaxAttemptsPerModel = parseInteger(draft.retryMaxAttemptsPerModel);
  const retryInitialBackoffMs = parseInteger(draft.retryInitialBackoffMs);
  const retryMaxBackoffMs = parseInteger(draft.retryMaxBackoffMs);
  const riskDailyProbeBudget = parseInteger(draft.riskDailyProbeBudget);
  const healthFailureThreshold = parseInteger(draft.healthFailureThreshold);
  const healthRecoveryThreshold = parseInteger(draft.healthRecoveryThreshold);
  const attemptTimeoutMs = parseInteger(draft.attemptTimeoutMs);
  const executionTimeoutMs = parseInteger(draft.executionTimeoutMs);

  if (!draft.name.trim()) {
    return "请输入监控名称";
  }
  if (!draft.stationId) {
    return "请选择中转站";
  }
  if (draft.targetType === "station_key" && !draft.stationKeyId) {
    return "请选择要检测的密钥";
  }
  if (draft.targetType === "station_key") {
    const selectedKey = keys.find((key) => key.id === draft.stationKeyId);
    if (!selectedKey) {
      return "所选密钥不存在，请重新选择";
    }
    if (selectedKey.stationId !== draft.stationId) {
      return "所选密钥不属于当前中转站，请重新选择";
    }
  }
  if (draft.targetType === "station" && draft.stationKeyId) {
    return "中转站目标不能绑定单个密钥";
  }
  if (!draft.templateId) {
    return templates.some((template) => template.enabled) ? "请选择启用的请求模板" : "暂无启用的请求模板";
  }
  const selectedTemplate = templates.find((template) => template.id === draft.templateId);
  if (!selectedTemplate) {
    return "所选请求模板不存在，请重新选择";
  }
  if (!selectedTemplate.enabled) {
    return "所选请求模板已停用，请选择启用模板";
  }
  if (!draft.primaryModel.trim()) {
    return "请输入检测模型";
  }
  if (draft.fallbackModels.filter((model) => model.trim()).length > 3) {
    return "最多配置 3 个回退模型";
  }
  const normalizedModels = [draft.primaryModel, ...draft.fallbackModels]
    .map((model) => model.trim().toLowerCase())
    .filter(Boolean);
  if (new Set(normalizedModels).size !== normalizedModels.length) {
    return "主模型和回退模型不能重复";
  }
  const selectedProtocol = capabilities?.protocols.find((protocol) => protocol.id === draft.protocolKind);
  if (!selectedProtocol || !selectedProtocol.enabled) {
    return capabilities ? "请选择可用的请求协议" : "正在加载监控能力";
  }
  const selectedProfile = capabilities?.profiles.find((profile) => profile.id === draft.clientProfileId);
  if (!selectedProfile || !selectedProfile.enabled) {
    return capabilities ? "请选择可用的请求 Profile" : "正在加载监控能力";
  }
  if (!selectedProfile.supportedProtocols.includes(draft.protocolKind)) {
    return "请求 Profile 不支持当前协议";
  }
  if (clientProfileVersion !== selectedProfile.version) {
    return "请求 Profile 版本已变化，请重新选择";
  }
  if (!isInRange(intervalSeconds, 15, 3600)) {
    return "检测间隔需在 15 到 3600 秒之间";
  }
  if (!isInRange(jitterSeconds, 0, 600)) {
    return "抖动需在 0 到 600 秒之间";
  }
  if (intervalSeconds !== null && jitterSeconds !== null && intervalSeconds - jitterSeconds < 15) {
    return "检测间隔减去抖动至少需要 15 秒";
  }
  if (!isInRange(attemptTimeoutMs, 1_000, 120_000)) return "单次请求超时需在 1000 到 120000 毫秒之间";
  if (!isInRange(executionTimeoutMs, 1_000, 300_000) || (attemptTimeoutMs ?? 0) >= (executionTimeoutMs ?? 0)) return "任务超时必须大于单次请求超时";
  if (!isInRange(retryMaxAttemptsPerModel, 1, 3)) return "每个模型尝试次数需在 1 到 3 之间";
  if (!isInRange(retryInitialBackoffMs, 0, 60_000) || !isInRange(retryMaxBackoffMs, 0, 60_000) || (retryMaxBackoffMs ?? 0) < (retryInitialBackoffMs ?? 0)) return "重试退避范围无效";
  if (!isInRange(riskDailyProbeBudget, 1, 10_000)) return "每日探测预算需在 1 到 10000 之间";
  if (!isInRange(healthFailureThreshold, 1, 20) || !isInRange(healthRecoveryThreshold, 1, 20)) return "健康阈值需在 1 到 20 之间";
  if (draft.healthPolicyMode === "authoritative" && draft.clientProfileId !== "standard_api") return "权威健康写回只能使用标准 API Profile";
  const proxyMode = draft.proxyMode ?? "inherit";
  const proxyUrl = draft.proxyUrl ?? "";
  if (proxyMode === "manual") {
    if (!proxyUrl.trim()) return "请输入代理地址";
    try {
      const url = new URL(proxyUrl.trim());
      if (!['http:', 'https:', 'socks5:', 'socks5h:'].includes(url.protocol) || url.username || url.password) {
        return "代理地址格式无效";
      }
    } catch {
      return "代理地址格式无效";
    }
  } else if (proxyUrl.trim()) {
    return "只有手动代理模式可以填写代理地址";
  }
  return null;
}

export function formatTargetLabel(
  targetType: ChannelMonitorTargetType,
  stationId: string,
  stationKeyId: string | null,
  stations: Station[],
  keys: KeyPoolItem[],
) {
  const station = stations.find((item) => item.id === stationId);
  if (targetType === "station") {
    return station ? `${station.name} · 全部启用密钥` : "未知中转站 · 全部启用密钥";
  }
  const key = keys.find((item) => item.id === stationKeyId);
  if (key) {
    return `${key.stationName} · ${key.name}`;
  }
  return station ? `${station.name} · 未选择密钥` : "未知密钥";
}

export function formatInterval(intervalSeconds: number, jitterSeconds: number) {
  const base = `每 ${formatDuration(intervalSeconds)}`;
  if (jitterSeconds <= 0) {
    return base;
  }
  return `${base} · 抖动 ${formatDuration(jitterSeconds)}`;
}

function formatDuration(seconds: number) {
  if (seconds % 60 === 0 && seconds >= 60) {
    const minutes = seconds / 60;
    return minutes >= 60 && minutes % 60 === 0 ? `${minutes / 60} 小时` : `${minutes} 分钟`;
  }
  return `${seconds} 秒`;
}

function parseInteger(value: string) {
  if (!/^\d+$/.test(value.trim())) {
    return null;
  }
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

function toInteger(value: string) {
  return parseInteger(value) ?? 0;
}

function isInRange(value: number | null, min: number, max: number) {
  return value !== null && value >= min && value <= max;
}

function toTime(value: string) {
  return toTimestampMillis(value);
}

function uniqueModels(models: string[]) {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const model of models) {
    const trimmed = model.trim();
    if (!trimmed) {
      continue;
    }
    const normalized = normalizeModelName(trimmed);
    if (seen.has(normalized)) {
      continue;
    }
    seen.add(normalized);
    result.push(trimmed);
  }
  return result;
}

function normalizeModelName(model: string) {
  return model.trim().toLowerCase();
}
