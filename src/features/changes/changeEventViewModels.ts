import type { ChangeEvent, ChangeEventStatus, ChangeSeverity } from "@/lib/types/changeEvents";
import type { StatusTone } from "@/components/ui";

export type ChangeFilter = {
  severity: "all" | ChangeSeverity;
  status: "active" | "all" | ChangeEventStatus;
  objectType: "all" | string;
  query: string;
};

export const severityLabels: Record<ChangeSeverity, string> = {
  critical: "严重",
  warning: "警告",
  info: "信息",
};

export const severityTone: Record<ChangeSeverity, StatusTone> = {
  critical: "error",
  warning: "warning",
  info: "info",
};

export const statusLabels: Record<ChangeEventStatus, string> = {
  unread: "未读",
  read: "已读",
  dismissed: "已忽略",
  resolved: "已解决",
};

export const eventTypeLabels: Record<string, string> = {
  balance_low: "余额偏低",
  balance_depleted: "余额耗尽",
  group_added: "分组新增",
  group_missing: "分组不可见",
  rate_changed: "倍率变化",
  price_changed: "价格变化",
  price_expired: "价格过期",
  model_added: "模型新增",
  model_removed: "模型下架",
  key_invalid: "密钥异常",
  key_group_bound: "密钥分组已绑定",
  key_group_unresolved: "密钥分组无法识别",
  collector_failed: "采集失败",
  collector_recovered: "采集恢复",
  route_impacted: "路由受影响",
  station_down: "站点异常",
  station_recovered: "站点恢复",
};

export const objectTypeLabels: Record<string, string> = {
  station: "中转站",
  station_key: "密钥",
  group_binding: "分组",
  pricing_rule: "价格",
  routing_rule: "路由规则",
  request_log: "请求",
  channel: "渠道状态",
  collector: "采集",
  collector_run: "采集",
  route: "路由",
};

export const sourceLabels: Record<string, string> = {
  balance: "余额",
  collector: "采集",
  health: "密钥",
  pricing: "价格",
  routing: "路由",
};

export type ChangeEventListDiff = {
  label: string;
  before: string | null;
  after: string | null;
};

export type ChangeEventListItem = {
  title: string;
  description: string;
  metaLabel: string;
  kindLabel: string;
  objectLabel: string;
  sourceLabel: string;
  statusLabel: string;
  severityLabel: string;
  diff: ChangeEventListDiff | null;
};

export function filterChangeEvents(events: ChangeEvent[], filter: ChangeFilter) {
  const query = filter.query.trim().toLowerCase();
  return events.filter((event) => {
    if (filter.severity !== "all" && event.severity !== filter.severity) {
      return false;
    }
    if (filter.status === "active") {
      if (event.status === "dismissed" || event.status === "resolved") {
        return false;
      }
    } else if (filter.status !== "all" && event.status !== filter.status) {
      return false;
    }
    if (filter.objectType !== "all" && event.objectType !== filter.objectType) {
      return false;
    }
    if (!query) {
      return true;
    }
    const item = buildChangeEventListItem(event);
    return `${event.title} ${event.message} ${event.eventType} ${event.source} ${event.objectType} ${item.title} ${item.description} ${item.metaLabel} ${item.objectLabel} ${item.diff?.before ?? ""} ${item.diff?.after ?? ""}`
      .toLowerCase()
      .includes(query);
  });
}

export function paginateChangeEvents(events: ChangeEvent[], page: number, pageSize: number) {
  const safePageSize = Math.max(1, pageSize);
  const totalPages = Math.max(1, Math.ceil(events.length / safePageSize));
  const safePage = Math.min(Math.max(1, page), totalPages);
  const startOffset = (safePage - 1) * safePageSize;
  const pageEvents = events.slice(startOffset, startOffset + safePageSize);

  return {
    events: pageEvents,
    page: safePage,
    pageSize: safePageSize,
    totalPages,
    startIndex: pageEvents.length === 0 ? 0 : startOffset + 1,
    endIndex: startOffset + pageEvents.length,
    totalCount: events.length,
  };
}

export function unreadRiskCount(events: ChangeEvent[]) {
  return events.filter(
    (event) => event.status === "unread" && (event.severity === "critical" || event.severity === "warning"),
  ).length;
}

export function unreadChangeCount(events: ChangeEvent[]) {
  return events.filter((event) => event.status === "unread").length;
}

export async function markUnreadChangeEventsRead(
  events: ChangeEvent[],
  markRead: (id: string) => Promise<ChangeEvent>,
) {
  const unreadEvents = events.filter((event) => event.status === "unread");
  const updatedEvents = await Promise.all(unreadEvents.map((event) => markRead(event.id)));
  const updatedById = new Map(updatedEvents.map((event) => [event.id, event]));

  return {
    changedCount: updatedEvents.length,
    events: events.map((event) => updatedById.get(event.id) ?? event),
  };
}

export function buildChangeEventListItem(event: ChangeEvent): ChangeEventListItem {
  const oldValue = parseJsonRecord(event.oldValueJson);
  const newValue = parseJsonRecord(event.newValueJson);
  const baseItem: ChangeEventListItem = {
    title: event.title,
    description: event.message,
    metaLabel: `${sourceLabels[event.source] ?? event.source} / ${objectTypeLabels[event.objectType] ?? event.objectType}`,
    kindLabel: eventTypeLabels[event.eventType] ?? event.eventType,
    objectLabel: objectTypeLabels[event.objectType] ?? event.objectType,
    sourceLabel: sourceLabels[event.source] ?? event.source,
    statusLabel: statusLabels[event.status] ?? event.status,
    severityLabel: severityLabels[event.severity] ?? event.severity,
    diff: buildGenericDiff(oldValue, newValue),
  };

  if (event.eventType === "rate_changed") {
    const groupName = readString(newValue, "groupName") ?? readString(oldValue, "groupName") ?? extractGroupName(event.message);
    const before = readMultiplier(oldValue);
    const after = readMultiplier(newValue);
    return {
      ...baseItem,
      title:
        groupName && (before != null || after != null)
          ? `分组 ${groupName} 倍率变化`
          : groupName
            ? `分组 ${groupName} 倍率未知`
            : event.title,
      diff: {
        label: "倍率",
        before: before == null ? null : `${formatCompactNumber(before)} 倍`,
        after: after == null ? null : `${formatCompactNumber(after)} 倍`,
      },
    };
  }

  if (event.eventType === "group_added") {
    const groupName = readString(newValue, "groupName") ?? extractGroupName(event.message);
    const multiplier = readMultiplier(newValue);
    return {
      ...baseItem,
      title: groupName
        ? `新增可用分组 ${groupName}，倍率 ${formatMultiplierLabel(multiplier)}`
        : `${event.title}，倍率 ${formatMultiplierLabel(multiplier)}`,
      metaLabel: `${baseItem.sourceLabel} / 分组`,
      diff: null,
    };
  }

  if (event.eventType === "group_missing") {
    const groupName = readString(newValue, "groupName") ?? readString(oldValue, "groupName") ?? extractGroupName(event.message);
    const multiplier = readMultiplier(newValue) ?? readMultiplier(oldValue);
    return {
      ...baseItem,
      title: groupName
        ? `分组 ${groupName} 不可见，倍率 ${formatMultiplierLabel(multiplier)}`
        : `${event.title}，倍率 ${formatMultiplierLabel(multiplier)}`,
      diff: {
        label: "状态",
        before: formatBindingStatus(readString(oldValue, "bindingStatus")),
        after: formatBindingStatus(readString(newValue, "bindingStatus")),
      },
    };
  }

  if (event.eventType === "price_changed") {
    const model = extractModelName(event.message);
    const currency = readString(newValue, "currency") ?? readString(oldValue, "currency");
    return {
      ...baseItem,
      title: model ? `模型 ${model} 价格变化` : event.title,
      diff: {
        label: "输出价格",
        before: formatPrice(readNumber(oldValue, "outputPrice"), currency),
        after: formatPrice(readNumber(newValue, "outputPrice"), currency),
      },
    };
  }

  if (event.eventType === "model_added" || event.eventType === "model_removed") {
    const model = readString(newValue, "model") ?? readString(oldValue, "model") ?? extractModelName(event.message);
    return {
      ...baseItem,
      title: model ? `${event.eventType === "model_added" ? "新增模型" : "下架模型"} ${model}` : event.title,
      diff: {
        label: "模型",
        before: event.eventType === "model_removed" ? model : null,
        after: event.eventType === "model_added" ? model : null,
      },
    };
  }

  if (event.eventType === "balance_low" || event.eventType === "balance_depleted") {
    const value = readNumber(newValue, "value");
    const threshold = readNumber(newValue, "threshold");
    return {
      ...baseItem,
      diff: {
        label: "余额",
        before: threshold == null ? null : `阈值 ${formatCompactNumber(threshold)}`,
        after: value == null ? null : formatCompactNumber(value),
      },
    };
  }

  if (event.eventType === "key_invalid") {
    const failures = readNumber(newValue, "consecutiveFailures");
    const stationKeyName = readString(newValue, "stationKeyName");
    const apiKeyMasked = readString(newValue, "apiKeyMasked");
    const stationKeyLabel =
      stationKeyName ?? apiKeyMasked ?? shortenIdentifier(event.stationKeyId ?? event.objectId ?? null) ?? baseItem.objectLabel;
    const descriptionLabel = apiKeyMasked ?? stationKeyLabel;
    const description = event.message.replace(/^Key\s+/, `${descriptionLabel} `);
    return {
      ...baseItem,
      title: `Key ${stationKeyLabel} 健康异常`,
      description,
      metaLabel: `${baseItem.sourceLabel} / ${stationKeyLabel}`,
      objectLabel: stationKeyLabel,
      diff: {
        label: "失败次数",
        before: null,
        after: failures == null ? null : `${formatCompactNumber(failures)} 次`,
      },
    };
  }

  return baseItem;
}

export function formatChangeTime(value: string) {
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

function buildGenericDiff(
  oldValue: Record<string, unknown> | null,
  newValue: Record<string, unknown> | null,
): ChangeEventListDiff | null {
  if (!oldValue && !newValue) {
    return null;
  }
  return {
    label: "变化",
    before: formatRecordSummary(oldValue),
    after: formatRecordSummary(newValue),
  };
}

function parseJsonRecord(value: string | null) {
  const parsed = parseJsonObject(value);
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    return null;
  }
  return parsed as Record<string, unknown>;
}

function readString(value: Record<string, unknown> | null, key: string) {
  const item = value?.[key];
  return typeof item === "string" && item.trim() ? item.trim() : null;
}

function readNumber(value: Record<string, unknown> | null, key: string) {
  const item = value?.[key];
  return typeof item === "number" && Number.isFinite(item) ? item : null;
}

function readMultiplier(value: Record<string, unknown> | null) {
  return (
    readNumber(value, "effectiveRateMultiplier") ??
    readNumber(value, "effective_rate_multiplier") ??
    readNumber(value, "multiplier") ??
    readNumber(value, "rateMultiplier") ??
    readNumber(value, "rate_multiplier") ??
    readNumber(value, "userRateMultiplier") ??
    readNumber(value, "user_rate_multiplier") ??
    readNumber(value, "defaultRateMultiplier") ??
    readNumber(value, "default_rate_multiplier")
  );
}

function formatCompactNumber(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
}

function formatMultiplierLabel(value: number | null) {
  return value == null ? "未知" : `${formatCompactNumber(value)} 倍`;
}

function formatPrice(value: number | null, currency: string | null) {
  if (value == null) {
    return null;
  }
  return `${currency ?? ""} ${formatCompactNumber(value)}`.trim();
}

function formatBindingStatus(value: string | null) {
  if (value === "available") {
    return "可用";
  }
  if (value === "missing") {
    return "不可见";
  }
  return value;
}

function formatRecordSummary(value: Record<string, unknown> | null) {
  if (!value) {
    return null;
  }
  const firstEntry = Object.entries(value).find(([, item]) => item != null);
  if (!firstEntry) {
    return null;
  }
  const [, item] = firstEntry;
  return typeof item === "number" ? formatCompactNumber(item) : String(item);
}

function shortenIdentifier(value: string | null) {
  if (!value) {
    return null;
  }
  return value.length > 18 ? `${value.slice(0, 15)}...` : value;
}

function extractGroupName(message: string) {
  const missingMatch = message.match(/^分组\s+(.+?)\s+在最新采集中不可见/);
  if (missingMatch?.[1]) {
    return missingMatch[1].trim();
  }
  const rateMatch = message.match(/^分组\s+(.+?)\s+倍率发生变化/);
  if (rateMatch?.[1]) {
    return rateMatch[1].trim();
  }
  const addedMatch = message.match(/分组\s+(.+)$/);
  return addedMatch?.[1]?.trim() ?? null;
}

function extractModelName(message: string) {
  const match = message.match(/^模型\s+(.+?)\s+(?:输出价格发生变化|的价格规则已过期|新增|下架)/);
  return match?.[1]?.trim() ?? null;
}

export function parseJsonObject(value: string | null) {
  if (!value) {
    return null;
  }
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return value;
  }
}
