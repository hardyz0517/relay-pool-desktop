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
  key_invalid: "Key 异常",
  key_group_bound: "Key 分组已绑定",
  key_group_unresolved: "Key 分组无法识别",
  collector_failed: "采集失败",
  collector_recovered: "采集恢复",
  route_impacted: "路由受影响",
  station_down: "站点异常",
  station_recovered: "站点恢复",
};

export const objectTypeLabels: Record<string, string> = {
  station: "中转站",
  station_key: "Key",
  group_binding: "分组",
  pricing_rule: "价格",
  routing_rule: "路由规则",
  request_log: "请求",
  channel: "渠道状态",
  collector: "采集",
  collector_run: "采集",
  route: "路由",
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
    return `${event.title} ${event.message} ${event.eventType} ${event.source} ${event.objectType}`
      .toLowerCase()
      .includes(query);
  });
}

export function unreadRiskCount(events: ChangeEvent[]) {
  return events.filter(
    (event) => event.status === "unread" && (event.severity === "critical" || event.severity === "warning"),
  ).length;
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
