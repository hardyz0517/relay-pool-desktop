import { formatTrimmedDecimal } from "@/lib/formatters";
import { toTimestampMillis } from "@/lib/time";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { BalanceSnapshot } from "@/lib/types/economics";
import { isCollectedStationGroupBinding, type GroupRateRecord, type StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKey } from "@/lib/types/stationKeys";
import type { StationCredentials } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";

export type DetailTone = "neutral" | "good" | "warning" | "error" | "muted";

export type StationDetailBalanceCard = {
  label: string;
  value: string;
  helper: string;
  tone: DetailTone;
};

export type StationDetailGroupRow = {
  id: string;
  groupName: string;
  effectiveRate: string;
  defaultRate: string;
  userRate: string;
  bindingStatus: string;
  sourceLabel: string;
  lastChecked: string;
  tone: DetailTone;
  warning: string | null;
};

export type StationDetailDiagnosticItem = {
  label: string;
  value: string;
  tone: DetailTone;
};

export type StationDetailViewModel = {
  station: Station;
  stationTypeLabel: string;
  statusLabel: string;
  statusTone: DetailTone;
  lastActivityLabel: string;
  balanceCards: StationDetailBalanceCard[];
  groupRows: StationDetailGroupRow[];
  groupEmptyMessage: string;
  loginItems: StationDetailDiagnosticItem[];
  collectorItems: StationDetailDiagnosticItem[];
  snapshotItems: StationDetailDiagnosticItem[];
  changeItems: StationDetailDiagnosticItem[];
};

const stationTypeLabels: Record<string, string> = {
  sub2api: "Sub2API",
  newapi: "NewAPI",
  "openai-compatible": "自定义接口",
  custom: "自定义接口",
};

const stationStatusLabels: Record<string, string> = {
  healthy: "采集正常",
  warning: "采集需关注",
  error: "采集异常",
  disabled: "禁用",
  unchecked: "未采集",
};

const bindingStatusLabels: Record<string, string> = {
  available: "可用",
  bound: "已绑定",
  missing: "缺失",
  disabled: "禁用",
  manual_legacy: "手动维护",
};

const collectorStatusLabels: Record<string, string> = {
  running: "运行中",
  success: "成功",
  partial: "部分成功",
  failed: "失败",
  manual_required: "需要人工处理",
};

const rateSourceLabels: Record<string, string> = {
  binding: "本地绑定",
  collector: "采集结果",
  manual: "手动维护",
  manual_legacy: "旧版手动维护",
  sub2api_groups_rates: "Sub2API 分组倍率接口",
  newapi_groups_rates: "NewAPI 分组倍率接口",
};

const balanceSourceLabels: Record<string, string> = {
  mock: "示例数据",
  station_config: "站点配置",
  station_balance: "站点余额接口",
  station_key_balance: "站点 Key 余额",
  station_key_balance_aggregate: "站点 Key 余额汇总",
  collector_snapshot: "采集快照",
};

export function formatStationTypeLabel(station: Station) {
  return stationTypeLabels[station.stationType] ?? station.stationType;
}

export function formatStationStatusLabel(station: Station) {
  if (!station.enabled) {
    return "禁用";
  }
  return stationStatusLabels[station.status] ?? station.status;
}

export function statusTone(station: Station): DetailTone {
  if (!station.enabled || station.status === "disabled") {
    return "muted";
  }
  if (station.status === "healthy") {
    return "good";
  }
  if (station.status === "warning") {
    return "warning";
  }
  if (station.status === "error") {
    return "error";
  }
  return "neutral";
}

export function formatBindingStatusLabel(status: string) {
  return bindingStatusLabels[status] ?? status;
}

export function formatDetailDate(value: string | null | undefined) {
  const time = toTime(value ?? null);
  if (time === 0) {
    return "未记录";
  }
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(time));
}

export function formatMoney(value: number | null | undefined, currency = "CNY") {
  if (value == null || !Number.isFinite(value)) {
    return "未采集";
  }
  return `${currency} ${value.toFixed(2)}`;
}

export function formatRate(value: number | null | undefined, fallback = "未采集") {
  if (value == null || !Number.isFinite(value)) {
    return fallback;
  }
  return `${trimFixed(value, 3)}x`;
}

export function latestByTime<T>(items: T[], selectTime: (item: T) => string | null | undefined) {
  let latest: T | null = null;
  let latestTime = 0;
  for (const item of items) {
    const time = toTime(selectTime(item) ?? null);
    if (!latest || time > latestTime) {
      latest = item;
      latestTime = time;
    }
  }
  return latest;
}

export function buildBalanceCards(station: Station, balances: BalanceSnapshot[]): StationDetailBalanceCard[] {
  const latestBalance = latestByTime(
    balances.filter((balance) => balance.stationId === station.id && balance.scope === "station"),
    (balance) => balance.updatedAt,
  );
  const currency = latestBalance?.currency ?? "CNY";
  const currentValue = latestBalance?.value ?? station.balanceCny;
  const threshold = latestBalance?.lowBalanceThreshold ?? station.lowBalanceThresholdCny;
  const balanceTone = balanceToneFor(currentValue, threshold, latestBalance?.status);

  return [
    {
      label: "当前余额",
      value: formatMoney(currentValue, currency),
      helper: latestBalance ? `来源：${formatBalanceSourceLabel(latestBalance.source)}` : "来自站点配置或尚未采集",
      tone: balanceTone,
    },
    {
      label: "低余额阈值",
      value: formatMoney(threshold, currency),
      helper: threshold == null ? "未设置阈值" : "低于该值时标记为风险",
      tone: threshold == null ? "muted" : "neutral",
    },
    {
      label: "余额更新时间",
      value: formatDetailDate(latestBalance?.updatedAt ?? station.lastCheckedAt),
      helper: latestBalance?.collectedAt
        ? `采集时间：${formatDetailDate(latestBalance.collectedAt)}`
        : "等待采集器写入余额快照",
      tone: latestBalance ? "neutral" : "muted",
    },
  ];
}

export function buildGroupRows(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
): StationDetailGroupRow[] {
  const stationGroupBindings = dedupeStationGroupBindings(bindings.filter(isCollectedStationGroupBinding), rates);
  const latestRateByBindingId = new Map<string, GroupRateRecord>();

  for (const rate of rates) {
    if (rate.bindingKind !== "station_group" || !rate.groupBindingId) {
      continue;
    }
    const current = latestRateByBindingId.get(rate.groupBindingId);
    if (!current || toTime(rate.checkedAt) > toTime(current.checkedAt)) {
      latestRateByBindingId.set(rate.groupBindingId, rate);
    }
  }

  return stationGroupBindings.map((binding) => {
    const latestRate = latestRateByBindingId.get(binding.id) ?? null;
    const effectiveRate = binding.effectiveRateMultiplier ?? latestRate?.effectiveRateMultiplier ?? null;
    const defaultRate = binding.defaultRateMultiplier ?? latestRate?.defaultRateMultiplier ?? null;
    const userRate = binding.userRateMultiplier ?? latestRate?.userRateMultiplier ?? null;
    const warning = groupWarningFor(binding, effectiveRate);
    const rateSource = binding.rateSource ?? latestRate?.source ?? "binding";

    return {
      id: binding.id,
      groupName: binding.groupName || latestRate?.groupName || "未命名分组",
      effectiveRate: formatRate(effectiveRate, "未确定"),
      defaultRate: formatRate(defaultRate),
      userRate: formatRate(userRate, "未覆盖"),
      bindingStatus: formatBindingStatusLabel(binding.bindingStatus),
      sourceLabel: formatRateSourceLabel(rateSource),
      lastChecked: formatDetailDate(binding.lastCheckedAt ?? latestRate?.checkedAt ?? binding.updatedAt),
      tone: warning ? "warning" : binding.bindingStatus === "available" || binding.bindingStatus === "bound" ? "good" : "neutral",
      warning,
    };
  });
}

function dedupeStationGroupBindings(
  bindings: StationGroupBinding[],
  rates: GroupRateRecord[],
): StationGroupBinding[] {
  const latestRateByBindingId = new Map<string, GroupRateRecord>();
  for (const rate of rates) {
    if (rate.bindingKind !== "station_group" || !rate.groupBindingId) {
      continue;
    }
    const current = latestRateByBindingId.get(rate.groupBindingId);
    if (!current || toTime(rate.checkedAt) > toTime(current.checkedAt)) {
      latestRateByBindingId.set(rate.groupBindingId, rate);
    }
  }

  const byGroupName = new Map<string, StationGroupBinding>();
  for (const binding of bindings) {
    const key = normalizeGroupName(binding.groupName);
    const existing = byGroupName.get(key);
    if (!existing) {
      byGroupName.set(key, binding);
      continue;
    }
    byGroupName.set(
      key,
      preferStationGroupBinding(
        existing,
        binding,
        latestRateByBindingId.get(existing.id) ?? null,
        latestRateByBindingId.get(binding.id) ?? null,
      ),
    );
  }
  return Array.from(byGroupName.values());
}

function preferStationGroupBinding(
  left: StationGroupBinding,
  right: StationGroupBinding,
  leftRate: GroupRateRecord | null,
  rightRate: GroupRateRecord | null,
) {
  const leftScore = stationGroupBindingScore(left, leftRate);
  const rightScore = stationGroupBindingScore(right, rightRate);
  if (rightScore !== leftScore) {
    return rightScore > leftScore ? right : left;
  }
  return toTime(right.lastCheckedAt ?? right.updatedAt) > toTime(left.lastCheckedAt ?? left.updatedAt)
    ? right
    : left;
}

function stationGroupBindingScore(binding: StationGroupBinding, latestRate: GroupRateRecord | null) {
  let score = 0;
  const source = binding.rateSource ?? latestRate?.source ?? "";
  if (source !== "remote_scan") {
    score += 10;
  }
  if (source.includes("groups_rates")) {
    score += 5;
  }
  if (
    binding.effectiveRateMultiplier != null ||
    binding.defaultRateMultiplier != null ||
    binding.userRateMultiplier != null ||
    latestRate?.effectiveRateMultiplier != null ||
    latestRate?.defaultRateMultiplier != null ||
    latestRate?.userRateMultiplier != null
  ) {
    score += 3;
  }
  return score;
}

function normalizeGroupName(value: string) {
  return value.trim().toLowerCase();
}

export function buildStationDetailViewModel({
  station,
  balances,
  groupBindings,
  groupRates,
  collectorRuns,
  latestSnapshot,
  credentials,
  stationKeys,
  changes,
}: {
  station: Station;
  balances: BalanceSnapshot[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  collectorRuns: CollectorRun[];
  latestSnapshot: CollectorSnapshot | null;
  credentials: StationCredentials | null;
  stationKeys: StationKey[];
  changes: ChangeEvent[];
}): StationDetailViewModel {
  const activeChanges = changes
    .filter(
      (event) =>
        event.stationId === station.id &&
        event.status !== "dismissed" &&
        event.status !== "resolved",
    )
    .sort((left, right) => toTime(right.detectedAt) - toTime(left.detectedAt));
  const stationRuns = collectorRuns.filter((run) => run.stationId === station.id);
  const latestRun = latestByTime(stationRuns, (run) => run.finishedAt ?? run.startedAt);
  const stationKeyTotalCount = stationKeys.filter((key) => key.stationId === station.id).length;
  const stationKeyEnabledCount = stationKeys.filter((key) => key.stationId === station.id && key.enabled).length;
  const latestActivity = latestTime([
    latestRun?.finishedAt,
    latestRun?.startedAt,
    latestSnapshot?.fetchedAt,
    station.lastCheckedAt,
    station.updatedAt,
  ]);

  return {
    station,
    stationTypeLabel: formatStationTypeLabel(station),
    statusLabel: formatStationStatusLabel(station),
    statusTone: statusTone(station),
    lastActivityLabel: formatDetailDate(latestActivity),
    balanceCards: buildBalanceCards(station, balances),
    groupRows: buildGroupRows(
      groupBindings.filter((binding) => binding.stationId === station.id),
      groupRates.filter((rate) => rate.stationId === station.id),
    ),
    groupEmptyMessage: "暂无站点分组倍率记录",
    loginItems: buildLoginItems(credentials, stationKeyEnabledCount, stationKeyTotalCount),
    collectorItems: buildCollectorItems(latestRun),
    snapshotItems: buildSnapshotItems(latestSnapshot),
    changeItems: buildChangeItems(activeChanges),
  };
}

function buildLoginItems(
  credentials: StationCredentials | null,
  stationKeyEnabledCount: number,
  stationKeyTotalCount: number,
): StationDetailDiagnosticItem[] {
  return [
    {
      label: "登录账号",
      value: credentials?.loginUsername ?? "未配置",
      tone: credentials?.loginUsername ? "neutral" : "muted",
    },
    {
      label: "登录密码",
      value: credentials?.passwordPresent ? "已保存" : "未保存",
      tone: credentials?.passwordPresent ? "good" : "muted",
    },
    {
      label: "站点 Key",
      value: `${stationKeyEnabledCount} / ${stationKeyTotalCount} 启用`,
      tone: stationKeyEnabledCount > 0 ? "good" : "warning",
    },
  ];
}

function buildCollectorItems(latestRun: CollectorRun | null): StationDetailDiagnosticItem[] {
  return [
    {
      label: "最近任务",
      value: latestRun ? `${formatCollectorTaskType(latestRun.taskType)} · ${formatCollectorStatus(latestRun.status)}` : "未运行",
      tone: collectorTone(latestRun?.status),
    },
    {
      label: "最近完成",
      value: formatDetailDate(latestRun?.finishedAt ?? latestRun?.startedAt),
      tone: latestRun ? "neutral" : "muted",
    },
    {
      label: "失败次数",
      value: latestRun ? String(latestRun.failureCount) : "0",
      tone: latestRun && latestRun.failureCount > 0 ? "warning" : "good",
    },
  ];
}

function buildSnapshotItems(snapshot: CollectorSnapshot | null): StationDetailDiagnosticItem[] {
  return [
    {
      label: "快照来源",
      value: snapshot?.source ?? "未采集",
      tone: snapshot ? "neutral" : "muted",
    },
    {
      label: "快照状态",
      value: snapshot?.status ?? "未采集",
      tone: snapshotTone(snapshot?.status),
    },
    {
      label: "快照时间",
      value: formatDetailDate(snapshot?.fetchedAt),
      tone: snapshot ? "neutral" : "muted",
    },
  ];
}

function buildChangeItems(changes: ChangeEvent[]): StationDetailDiagnosticItem[] {
  if (changes.length === 0) {
    return [
      {
        label: "活跃变更",
        value: "暂无",
        tone: "good",
      },
    ];
  }

  return changes.slice(0, 4).map((event) => ({
    label: event.title,
    value: event.message,
    tone: event.severity === "critical" ? "error" : event.severity === "warning" ? "warning" : "neutral",
  }));
}

function balanceToneFor(
  value: number | null | undefined,
  threshold: number | null | undefined,
  status: string | null | undefined,
): DetailTone {
  if (status === "depleted") {
    return "error";
  }
  if (value == null || !Number.isFinite(value)) {
    return "muted";
  }
  if (value <= 0) {
    return "error";
  }
  if (status === "low" || (threshold != null && Number.isFinite(threshold) && value <= threshold)) {
    return "warning";
  }
  return "good";
}

function groupWarningFor(binding: StationGroupBinding, effectiveRate: number | null) {
  if (binding.bindingStatus === "missing") {
    return "分组缺失";
  }
  if (effectiveRate == null || !Number.isFinite(effectiveRate)) {
    return "缺少倍率";
  }
  if (effectiveRate === 0) {
    return "倍率为 0";
  }
  return null;
}

function collectorTone(status: string | null | undefined): DetailTone {
  if (!status) {
    return "muted";
  }
  if (status === "success") {
    return "good";
  }
  if (status === "partial" || status === "manual_required" || status === "running") {
    return "warning";
  }
  if (status === "failed") {
    return "error";
  }
  return "neutral";
}

function snapshotTone(status: string | null | undefined): DetailTone {
  if (!status) {
    return "muted";
  }
  if (status === "success" || status === "normal") {
    return "good";
  }
  if (status === "partial" || status === "warning") {
    return "warning";
  }
  if (status === "failed" || status === "error") {
    return "error";
  }
  return "neutral";
}

function formatCollectorStatus(status: string) {
  return collectorStatusLabels[status] ?? status;
}

function formatRateSourceLabel(source: string) {
  return rateSourceLabels[source] ?? source.replace(/_/g, " ");
}

function formatBalanceSourceLabel(source: string) {
  return balanceSourceLabels[source] ?? source.replace(/_/g, " ");
}

function formatCollectorTaskType(taskType: string) {
  const labels: Record<string, string> = {
    detect: "探测",
    balance: "余额",
    groups: "分组",
    models: "模型",
    full: "全量",
  };
  return labels[taskType] ?? taskType;
}

function trimFixed(value: number, digits: number) {
  return formatTrimmedDecimal(value, digits);
}

function latestTime(values: Array<string | null | undefined>) {
  let latestValue: string | null = null;
  let latestTimeValue = 0;
  for (const value of values) {
    const time = toTime(value ?? null);
    if (time > latestTimeValue) {
      latestValue = value ?? null;
      latestTimeValue = time;
    }
  }
  return latestValue;
}

function toTime(value: string | null) {
  if (!value) {
    return 0;
  }
  const time = toTimestampMillis(value);
  return Number.isNaN(time) ? 0 : time;
}
