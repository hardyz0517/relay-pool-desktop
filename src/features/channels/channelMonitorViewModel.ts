import type { StatusTone } from "@/components/ui";
import type {
  ChannelMonitor,
  ChannelMonitorRequestTemplate,
  ChannelMonitorRunStatus,
  ChannelMonitorTargetType,
  CreateChannelMonitorInput,
} from "@/lib/types/channelMonitors";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export type ChannelMonitorDraft = {
  name: string;
  targetType: ChannelMonitorTargetType;
  stationId: string;
  stationKeyId: string;
  templateId: string;
  enabled: boolean;
  intervalSeconds: string;
  jitterSeconds: string;
  timeoutSeconds: string;
  maxConcurrency: string;
  consecutiveFailureThreshold: string;
  detectionModel: string;
  extraFallbackModels: string[];
  note: string;
};

export type RunStatusView = {
  label: string;
  tone: StatusTone;
};

type MonitorValidationContext = {
  templates: ChannelMonitorRequestTemplate[];
  keys: KeyPoolItem[];
};

type StationKeyMonitorTemplatePreference = {
  stationType?: string | null;
  stationUpstreamApiFormat?: string | null;
  capabilities?: Pick<StationKeyCapabilities, "supportsChatCompletions" | "supportsResponses"> | null;
};

export type ChannelMonitorProtocol = "chat_completions" | "responses";

export const DEFAULT_STATION_KEY_MONITOR_MODEL = "gpt-4o-mini";
export const DEFAULT_STATION_KEY_MONITOR_TEMPLATE_ID = "builtin-openai-responses-low-token";
export const STATION_KEY_MONITOR_NOTE = "由密钥池监控开关创建";

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

export function protocolForMonitorTemplate(
  templateId: string,
  templates: Array<Pick<ChannelMonitorRequestTemplate, "id" | "endpointKind">>,
): ChannelMonitorProtocol {
  const endpointKind = templates.find((template) => template.id === templateId)?.endpointKind;
  return endpointKind === "responses" ? "responses" : "chat_completions";
}

export function monitorTemplateOptionsForProtocol<T extends Pick<ChannelMonitorRequestTemplate, "endpointKind">>(
  templates: T[],
  protocol: ChannelMonitorProtocol,
) {
  return templates.filter((template) => template.endpointKind === protocol);
}

export function selectStationKeyMonitorModel(
  capabilities?: Pick<StationKeyCapabilities, "modelAllowlist" | "modelBlocklist" | "preferredModels"> | null,
) {
  const blockedModels = new Set((capabilities?.modelBlocklist ?? []).map(normalizeModelName));
  const explicitModels = capabilities?.modelAllowlist?.length
    ? capabilities.modelAllowlist
    : capabilities?.preferredModels ?? [];
  const candidates = uniqueModels(explicitModels).filter((model) => !blockedModels.has(normalizeModelName(model)));
  const selected = candidates.sort(compareMonitorModelPriority)[0];
  return selected ?? (blockedModels.has(normalizeModelName(DEFAULT_STATION_KEY_MONITOR_MODEL))
    ? candidates[0] ?? DEFAULT_STATION_KEY_MONITOR_MODEL
    : DEFAULT_STATION_KEY_MONITOR_MODEL);
}

export function createStationKeyMonitorInput(
  key: Pick<KeyPoolItem, "id" | "stationId" | "name">,
  template: Pick<ChannelMonitorRequestTemplate, "id">,
  capabilities?: Pick<StationKeyCapabilities, "modelAllowlist" | "modelBlocklist" | "preferredModels"> | null,
  testedModel?: string | null,
): CreateChannelMonitorInput {
  const fallbackModel = testedModel?.trim() || selectStationKeyMonitorModel(capabilities);
  return {
    name: `${key.name} 监控`,
    targetType: "station_key",
    stationId: key.stationId,
    stationKeyId: key.id,
    templateId: template.id,
    enabled: true,
    intervalSeconds: 300,
    jitterSeconds: 15,
    timeoutSeconds: 30,
    maxConcurrency: 1,
    consecutiveFailureThreshold: 3,
    fallbackModels: [fallbackModel],
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
    intervalSeconds: monitor.intervalSeconds,
    jitterSeconds: monitor.jitterSeconds,
    timeoutSeconds: monitor.timeoutSeconds,
    maxConcurrency: monitor.maxConcurrency,
    consecutiveFailureThreshold: monitor.consecutiveFailureThreshold,
    fallbackModels: [...monitor.fallbackModels],
    note: monitor.note,
  };
}

export function createEmptyMonitorDraft(stations: Station[] = [], templates: ChannelMonitorRequestTemplate[] = []): ChannelMonitorDraft {
  const stationId = stations[0]?.id ?? "";
  return {
    name: "",
    targetType: "station_key",
    stationId,
    stationKeyId: "",
    templateId: templates.find((template) => template.enabled)?.id ?? "",
    enabled: true,
    intervalSeconds: "60",
    jitterSeconds: "0",
    timeoutSeconds: "30",
    maxConcurrency: "2",
    consecutiveFailureThreshold: "3",
    detectionModel: "",
    extraFallbackModels: [],
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
    intervalSeconds: String(monitor.intervalSeconds),
    jitterSeconds: String(monitor.jitterSeconds),
    timeoutSeconds: String(monitor.timeoutSeconds),
    maxConcurrency: String(monitor.maxConcurrency),
    consecutiveFailureThreshold: String(monitor.consecutiveFailureThreshold),
    detectionModel: monitor.fallbackModels[0] ?? "",
    extraFallbackModels: monitor.fallbackModels.slice(1),
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
  const detectionModel = draft.detectionModel.trim();
  const extraFallbackModels = draft.extraFallbackModels
    .map((model) => model.trim())
    .filter((model) => model && model !== detectionModel);
  return {
    name: draft.name.trim(),
    targetType: draft.targetType,
    stationId: draft.stationId,
    stationKeyId: draft.targetType === "station_key" ? draft.stationKeyId : null,
    templateId: draft.templateId,
    enabled: draft.enabled,
    intervalSeconds: toInteger(draft.intervalSeconds),
    jitterSeconds: toInteger(draft.jitterSeconds),
    timeoutSeconds: toInteger(draft.timeoutSeconds),
    maxConcurrency: toInteger(draft.maxConcurrency),
    consecutiveFailureThreshold: toInteger(draft.consecutiveFailureThreshold),
    fallbackModels: [detectionModel, ...extraFallbackModels],
    note: draft.note.trim() ? draft.note.trim() : null,
  };
}

export function validateMonitorDraft(
  draft: ChannelMonitorDraft,
  { templates, keys }: MonitorValidationContext,
): string | null {
  const intervalSeconds = parseInteger(draft.intervalSeconds);
  const jitterSeconds = parseInteger(draft.jitterSeconds);
  const timeoutSeconds = parseInteger(draft.timeoutSeconds);
  const maxConcurrency = parseInteger(draft.maxConcurrency);
  const consecutiveFailureThreshold = parseInteger(draft.consecutiveFailureThreshold);

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
  if (!draft.detectionModel.trim()) {
    return "请输入检测模型";
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
  if (!isInRange(timeoutSeconds, 5, 120)) {
    return "超时时间需在 5 到 120 秒之间";
  }
  if (draft.targetType === "station" && !isInRange(maxConcurrency, 1, 10)) {
    return "中转站目标的最大并发需在 1 到 10 之间";
  }
  if (!isInRange(consecutiveFailureThreshold, 1, 20)) {
    return "连续失败阈值需在 1 到 20 之间";
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

export function formatTemplateLabel(template: ChannelMonitorRequestTemplate | undefined) {
  if (!template) {
    return "未知模板";
  }
  const state = template.enabled ? "" : " · 已停用";
  const builtIn = template.builtIn ? " · 内置" : "";
  return `${template.name}${builtIn}${state}`;
}

export function formatInterval(intervalSeconds: number, jitterSeconds: number) {
  const base = `每 ${formatDuration(intervalSeconds)}`;
  if (jitterSeconds <= 0) {
    return base;
  }
  return `${base} · 抖动 ${formatDuration(jitterSeconds)}`;
}

export function formatRunTimestamp(value: string | null) {
  if (!value) {
    return "未运行";
  }
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function getRunStatusView(status: ChannelMonitorRunStatus | null): RunStatusView {
  if (status === "success") {
    return { label: "成功", tone: "healthy" };
  }
  if (status === "warning") {
    return { label: "警告", tone: "warning" };
  }
  if (status === "failed") {
    return { label: "失败", tone: "error" };
  }
  if (status === "skipped") {
    return { label: "跳过", tone: "disabled" };
  }
  return { label: "未运行", tone: "info" };
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
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
  return date.getTime();
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

function compareMonitorModelPriority(left: string, right: string) {
  return monitorModelPriority(left) - monitorModelPriority(right);
}

function monitorModelPriority(model: string) {
  const normalized = normalizeModelName(model);
  if (normalized.includes("nano")) return 0;
  if (normalized.includes("mini")) return 1;
  if (normalized.includes("lite")) return 2;
  if (normalized.includes("flash")) return 3;
  if (normalized.includes("haiku")) return 4;
  if (normalized.includes("turbo")) return 5;
  if (normalized === "deepseek-chat" || normalized.endsWith("-chat")) return 6;
  return 20;
}

function normalizeModelName(model: string) {
  return model.trim().toLowerCase();
}
