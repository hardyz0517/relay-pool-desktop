import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Coins, Image, RefreshCw, RotateCcw, ShieldCheck, TrendingDown } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  EmptyState,
  MetricCard,
  SectionCard,
  SelectControl,
  Toolbar,
  useToast,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import { formatTrimmedDecimal } from "@/lib/formatters";
import { parseTimestampLikeDate } from "@/lib/time";
import { openStationWebsite } from "@/lib/api/stations";
import { Sub2ApiPlatformIcon } from "@/components/group/Sub2ApiPlatformIcon";
import { groupVisualMetaFor } from "@/lib/groupVisualMeta";
import { groupVisualClassNames } from "@/lib/groupVisualStyles";
import { groupCategoryDefinitions } from "@/lib/groupCategories";
import {
  pricingComparisonQueryOptions,
  pricingGroupMonitorStatusQueryOptions,
} from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { cn } from "@/lib/utils";
import {
  buildPricingComparisonViewModel,
  buildPricingMonitorRefs,
  type PricingComparisonRow,
  type PricingComparisonViewModel,
  type PricingGroupSection,
  type PricingGroupType,
} from "./pricingComparisonViewModel";
import {
  hashCanonicalPricingGroupRefs,
  type PricingGroupRefInput,
} from "@/lib/projections/pricingGroupRefs";
import type { PricingGroupMonitorStatusInput } from "@/lib/types/pricingMonitoring";
import type { RoutingDeepLink } from "@/lib/types/routingDeepLinks";
import { buildPricingMonitoringDeepLink } from "./pricingMonitoringDeepLink";

type GroupTypeFilter = PricingGroupType | "all";
type EmptyReason = PricingComparisonViewModel["emptyReason"];
type KeyPresenceFilter = "all" | "with_key" | "with_credentialed_key";
type MonitorPresenceFilter = "all" | "monitored" | "unmonitored";
type MonitorOutcomeFilter =
  | "all"
  | "success"
  | "degraded"
  | "failure"
  | "skipped"
  | "running"
  | "untested"
  | "unavailable_data"
  | "unresolved";

const groupTypeFilterOptions: Array<{ value: GroupTypeFilter; label: string }> = [
  { value: "all", label: "全部" },
];

function visibleGroupTypeFilterOptions(developerModeEnabled: boolean): Array<{ value: GroupTypeFilter; label: string }> {
  return [
    ...groupTypeFilterOptions,
    ...groupCategoryDefinitions
      .filter(
        (definition) =>
          developerModeEnabled || (definition.value !== "embedding" && definition.value !== "rerank"),
      )
      .map((definition) => ({
        value: definition.value,
        label: definition.label,
      })),
  ];
}

type PricingPageProps = {
  onOpenModelBasePrices: () => void;
  onOpenRoutingDeepLink?: (link: RoutingDeepLink) => void;
};

export function PricingPage({ onOpenModelBasePrices, onOpenRoutingDeepLink }: PricingPageProps) {
  const toast = useToast();
  const pricingQuery = useActivityQuery(
    pricingComparisonQueryOptions(),
  );
  const workspace = pricingQuery.data;
  const pricingRules = workspace?.pricingRules ?? [];
  const stations = workspace?.stations ?? [];
  const stationKeys = workspace?.stationKeys ?? [];
  const groupBindings = workspace?.groupBindings ?? [];
  const groupRates = workspace?.groupRates ?? [];
  const developerModeEnabled = workspace?.developerModeEnabled ?? false;
  const loading = pricingQuery.isPending && workspace === undefined;
  const error = pricingQuery.error ? readError(pricingQuery.error) : null;
  const [groupTypeFilter, setGroupTypeFilter] = useState<GroupTypeFilter>("all");
  const [query, setQuery] = useState("");
  const [selectedStationId, setSelectedStationId] = useState<string>("all");
  const [keyPresenceFilter, setKeyPresenceFilter] = useState<KeyPresenceFilter>("all");
  const [monitorPresenceFilter, setMonitorPresenceFilter] = useState<MonitorPresenceFilter>("all");
  const [monitorOutcomeFilter, setMonitorOutcomeFilter] = useState<MonitorOutcomeFilter>("all");
  const [monitorInput, setMonitorInput] = useState<PricingGroupMonitorStatusInput | null>(null);

  const monitorRefs = useMemo<PricingGroupRefInput[]>(
    () =>
      buildPricingMonitorRefs({
        stations,
        stationKeys,
        groupBindings,
        groupRates,
        pricingRules,
        developerModeEnabled,
      }),
    [developerModeEnabled, groupBindings, groupRates, pricingRules, stationKeys, stations],
  );

  useEffect(() => {
    let cancelled = false;
    if (monitorRefs.length === 0) {
      setMonitorInput(null);
      return () => {
        cancelled = true;
      };
    }
    void hashCanonicalPricingGroupRefs(monitorRefs)
      .then((groupRefsHash) => {
        if (!cancelled) {
          setMonitorInput({
            schemaVersion: 1,
            groupRefsHash,
            groups: monitorRefs,
          });
        }
      })
      .catch(() => {
        if (!cancelled) {
          setMonitorInput(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [monitorRefs]);

  const monitorQuery = useActivityQuery(
    pricingGroupMonitorStatusQueryOptions(
      monitorInput ?? {
        schemaVersion: 1,
        groupRefsHash: "",
        groups: [],
      },
      monitorInput !== null,
    ),
  );

  async function refresh(showSuccess = false) {
    try {
      await pricingQuery.refetch({ throwOnError: true });
      if (monitorInput !== null) {
        // Monitoring is an optional read projection. A stale/unavailable
        // summary must not make the price refresh look like a failure.
        await monitorQuery.refetch({ throwOnError: false });
      }
      if (showSuccess) {
        toast.success("价格倍率已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      toast.error("刷新价格倍率失败", message);
    }
  }

  const viewModel = useMemo(
    () =>
      buildPricingComparisonViewModel({
        stations,
        stationKeys,
        groupBindings,
        groupRates,
        pricingRules,
        developerModeEnabled,
        filters: {
          groupType: groupTypeFilter,
          query,
          stationId: selectedStationId,
          keyPresence: keyPresenceFilter,
          monitorPresence: monitorPresenceFilter,
          monitorOutcome: monitorOutcomeFilter,
        },
        monitorWorkspace: monitorQuery.data ?? null,
        monitorDataState:
          monitorQuery.isError ? "error" : monitorQuery.isPending ? "loading" : "ready",
      }),
    [
      groupBindings,
      groupRates,
      groupTypeFilter,
      developerModeEnabled,
      pricingRules,
      query,
      selectedStationId,
      keyPresenceFilter,
      monitorPresenceFilter,
      monitorOutcomeFilter,
      monitorQuery.data,
      monitorQuery.isError,
      monitorQuery.isPending,
      stationKeys,
      stations,
    ],
  );
  const stationWebsites = useMemo(
    () => new Map(stations.map((station) => [station.id, station.websiteUrl])),
    [stations],
  );

  async function handleOpenStation(stationId: string, stationName: string) {
    const websiteUrl = stationWebsites.get(stationId);
    if (!websiteUrl) {
      toast.error("打开中转站网址失败", `未找到 ${stationName} 的配置地址`);
      return;
    }

    try {
      await openStationWebsite(websiteUrl);
    } catch (error) {
      toast.error("打开中转站网址失败", readError(error));
    }
  }

  function resetFilters() {
    setGroupTypeFilter("all");
    setQuery("");
    setSelectedStationId("all");
    setKeyPresenceFilter("all");
    setMonitorPresenceFilter("all");
    setMonitorOutcomeFilter("all");
  }

  return (
    <PageScaffold
      title="价格 / 倍率"
      actions={
        <Button variant="secondary" onClick={onOpenModelBasePrices}>
          <Coins className="h-4 w-4" />
          模型基准价格
        </Button>
      }
    >
      <div className="grid gap-[var(--shell-page-gap)] md:grid-cols-2">
        <MetricCard
          className="!shadow-none"
          icon={ShieldCheck}
          label="可比分组"
          value={`${viewModel.metrics.comparableGroupCount}`}
          detail="已采集并可折算的分组倍率"
        />
        <MetricCard
          className="!shadow-none"
          icon={TrendingDown}
          label="最低倍率"
          value={
            viewModel.metrics.lowestEffectiveMultiplier === null
              ? "暂无"
              : formatMultiplier(viewModel.metrics.lowestEffectiveMultiplier)
          }
          detail={viewModel.metrics.lowestEffectiveMultiplierLabel || "暂无可比分组"}
          tone={viewModel.metrics.lowestEffectiveMultiplier === null ? "neutral" : "good"}
        />
      </div>

      <SectionCard
        title="分组倍率比较"
        contentClassName="overflow-visible rounded-none border-0 bg-transparent p-0 !shadow-none"
      >
        <Toolbar className="mb-4 items-end rounded-[var(--surface-radius)] border bg-surface px-4 py-3 !shadow-none">
          <div className="grid w-full grid-cols-1 items-end gap-3 sm:grid-cols-2 lg:grid-cols-4 2xl:grid-cols-[repeat(6,minmax(0,1fr))_auto]">
            <FilterField label="分组类型">
              <SelectControl
                ariaLabel="按分组类型筛选"
                className="w-full !shadow-none"
                value={groupTypeFilter}
                options={visibleGroupTypeFilterOptions(developerModeEnabled)}
                onChange={setGroupTypeFilter}
              />
            </FilterField>
            <FilterField label="搜索">
              <input
                id="pricing-group-search"
                aria-label="搜索中转站、Key 或分组"
                className={`${inputClassName} w-full`}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="中转站 / Key / 分组"
              />
            </FilterField>
            <FilterField label="中转站">
              <SelectControl
                ariaLabel="按中转站筛选"
                className="w-full !shadow-none"
                value={selectedStationId}
                options={[
                  { value: "all", label: "全部中转站" },
                  ...stations.map((station) => ({ value: station.id, label: station.name })),
                ]}
                onChange={setSelectedStationId}
              />
            </FilterField>
            <FilterField label="Key">
              <SelectControl
                ariaLabel="Key 筛选"
                className="w-full !shadow-none"
                value={keyPresenceFilter}
                options={[
                  { value: "all", label: "全部 Key" },
                  { value: "with_key", label: "仅有 Key" },
                  { value: "with_credentialed_key", label: "仅有凭据 Key" },
                ]}
                onChange={setKeyPresenceFilter}
              />
            </FilterField>
            <FilterField label="监控">
              <SelectControl
                ariaLabel="监控存在性筛选"
                className="w-full !shadow-none"
                value={monitorPresenceFilter}
                options={[
                  { value: "all", label: "全部监控" },
                  { value: "monitored", label: "仅有监控" },
                  { value: "unmonitored", label: "无监控" },
                ]}
                onChange={setMonitorPresenceFilter}
              />
            </FilterField>
            <FilterField label="监控结果">
              <SelectControl
                ariaLabel="监控结果筛选"
                className="w-full !shadow-none"
                value={monitorOutcomeFilter}
                options={[
                  { value: "all", label: "全部结果" },
                  { value: "success", label: "仅正常" },
                  { value: "degraded", label: "仅降级" },
                  { value: "failure", label: "仅失败" },
                  { value: "skipped", label: "仅跳过" },
                  { value: "running", label: "运行中" },
                  { value: "untested", label: "未测试" },
                  { value: "unavailable_data", label: "摘要暂不可用" },
                  { value: "unresolved", label: "无法解析" },
                ]}
                onChange={setMonitorOutcomeFilter}
              />
            </FilterField>
            <div className="flex items-center justify-end gap-2 sm:col-span-2 lg:col-span-2 2xl:col-span-1">
              <Button variant="secondary" disabled={pricingQuery.isFetching} onClick={() => void refresh(true)}>
                <RefreshCw className="h-4 w-4" />
                刷新
              </Button>
              <Button variant="secondary" onClick={resetFilters}>
                <RotateCcw className="h-4 w-4" />
                重置
              </Button>
            </div>
          </div>
        </Toolbar>

        <div className="space-y-4">
          {error && (
            <div className="rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
              {error}
            </div>
          )}

          {loading ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface px-4 py-5 text-sm text-muted-foreground">
              正在读取分组倍率...
            </div>
          ) : viewModel.sections.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-4">
              <PricingEmptyState reason={viewModel.emptyReason} />
            </div>
          ) : (
            <div className="space-y-4">
              {viewModel.sections.map((section) => (
                <GroupPricingSection
                  key={section.groupType}
                  section={section}
                  onOpenStation={handleOpenStation}
                  onOpenRoutingDeepLink={onOpenRoutingDeepLink}
                />
              ))}
            </div>
          )}
        </div>
      </SectionCard>
    </PageScaffold>
  );
}

function GroupPricingSection({
  section,
  onOpenStation,
  onOpenRoutingDeepLink,
}: {
  section: PricingGroupSection;
  onOpenStation: (stationId: string, stationName: string) => void;
  onOpenRoutingDeepLink?: (link: RoutingDeepLink) => void;
}) {
  return (
    <section className="grid gap-3 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface p-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          {section.groupType === "image_generation" && (
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md border border-border bg-surface text-muted-foreground">
              <Image className="h-4 w-4" />
            </span>
          )}
          <h3 className="text-sm font-semibold text-foreground">{section.title}</h3>
        </div>
        <div className="text-xs text-muted-foreground">{section.rows.length} 个分组</div>
      </div>

      <PricingRowsTable
        rows={section.rows}
        onOpenStation={onOpenStation}
        onOpenRoutingDeepLink={onOpenRoutingDeepLink}
      />
    </section>
  );
}

function PricingRowsTable({
  rows,
  onOpenStation,
  onOpenRoutingDeepLink,
}: {
  rows: PricingComparisonRow[];
  onOpenStation: (stationId: string, stationName: string) => void;
  onOpenRoutingDeepLink?: (link: RoutingDeepLink) => void;
}) {
  return (
    <div className={tableScrollClassName}>
      <table className={tableClassName}>
        <colgroup>
          <col className="w-[28%]" />
          <col className="w-[38%]" />
          <col className="w-[16%]" />
          <col className="w-[16%]" />
          <col className="w-[18%]" />
        </colgroup>
        <thead>
          <tr className="border-b border-border">
            <th className={tableHeaderClassName}>中转站</th>
            <th className={tableHeaderClassName}>分组</th>
            <th className={tableHeaderClassName}>状态</th>
            <th className={tableHeaderClassName}>倍率</th>
            <th className={updatedAtHeaderClassName}>最后变更时间</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {rows.map((row) => (
            <tr key={row.id}>
              <td className={tableCellClassName}>
                <button
                  type="button"
                  aria-label={`在浏览器打开 ${row.stationName}`}
                  title={`打开 ${row.stationName}`}
                  className="max-w-full truncate text-left font-medium text-foreground transition-colors hover:text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
                  onClick={() => onOpenStation(row.stationId, row.stationName)}
                >
                  {row.stationName}
                </button>
              </td>
              <td className={tableCellClassName}>
                <PricingGroupBadge row={row} />
              </td>
              <td className={tableCellClassName}>
                <PricingMonitorStatus row={row} onOpenRoutingDeepLink={onOpenRoutingDeepLink} />
              </td>
              <td className={`${tableCellClassName} tabular-nums font-semibold text-foreground`}>
                {formatNullableMultiplier(row.effectiveMultiplier)}
              </td>
              <td className={updatedAtCellClassName}>
                {formatTime(row.checkedAt)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function PricingMonitorStatus({
  row,
  onOpenRoutingDeepLink,
}: {
  row: PricingComparisonRow;
  onOpenRoutingDeepLink?: (link: RoutingDeepLink) => void;
}) {
  const meta: Record<PricingComparisonRow["monitorDisplayState"], { label: string; className: string }> = {
    unresolved: { label: "无法解析", className: "border-warning-border bg-warning-surface text-warning-foreground" },
    no_key: { label: "无 Key", className: "border-border bg-muted text-muted-foreground" },
    unmonitored: { label: "无监控", className: "border-border bg-muted text-muted-foreground" },
    running: { label: "运行中", className: "border-info-border bg-info-surface text-info-foreground" },
    untested: { label: "未测试", className: "border-border bg-muted text-muted-foreground" },
    available: { label: "正常", className: "border-success-border bg-success-surface text-success-foreground" },
    degraded: { label: "降级", className: "border-warning-border bg-warning-surface text-warning-foreground" },
    unavailable: { label: "失败", className: "border-danger-border bg-danger-surface text-danger-foreground" },
    skipped: { label: "跳过", className: "border-border bg-muted text-muted-foreground" },
    unavailable_data: { label: "暂不可用", className: "border-warning-border bg-warning-surface text-warning-foreground" },
  };
  const value = meta[row.monitorDisplayState];
  const deepLink = buildPricingMonitoringDeepLink(row);
  const content = (
    <span
      className={`inline-flex rounded-md border px-2 py-0.5 text-xs font-medium ${value.className}`}
      title={row.monitorSummary?.latestTerminalReason ?? undefined}
    >
      {value.label}
    </span>
  );
  return onOpenRoutingDeepLink && deepLink ? (
    <button
      type="button"
      className="rounded-md text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
      aria-label={`查看 ${row.stationName} 的监控定位`}
      onClick={() => onOpenRoutingDeepLink(deepLink)}
    >
      {content}
    </button>
  ) : (
    content
  );
}

function PricingGroupBadge({ row }: { row: PricingComparisonRow }) {
  const visualMeta = groupVisualMetaFor(row.groupName, row.groupRawJsonRedacted, row.groupType);
  const visualClassNames = groupVisualClassNames[visualMeta.platform];

  return (
    <span
      className={cn(
        "inline-flex h-6 max-w-full items-center gap-1.5 rounded-md border px-2 text-xs font-semibold",
        visualClassNames.badge,
      )}
      title={`${visualMeta.label} · ${row.groupName}`}
    >
      <Sub2ApiPlatformIcon platform={visualMeta.platform} className={visualClassNames.icon} />
      <span className="truncate">{row.groupName}</span>
    </span>
  );
}

function PricingEmptyState({ reason }: { reason: EmptyReason }) {
  if (reason === "no_group_rates") {
    return (
      <EmptyState
        title="暂无分组倍率"
        description="先采集中转站分组与倍率记录，再按分组类型比较倍率。"
      />
    );
  }

  if (reason === "filtered_empty") {
    return (
      <EmptyState
        title="没有匹配的分组"
        description="调整分组类型、关键词或中转站后再试。"
      />
    );
  }

  return (
    <EmptyState
      title="暂无分组倍率"
      description="采集分组倍率后，这里会显示同类分组的倍率比较。"
    />
  );
}

function FilterField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid min-w-0 gap-1.5 text-xs font-medium text-muted-foreground">
      <span>{label}</span>
      {children}
    </label>
  );
}

function formatNullableMultiplier(value: number | null) {
  return value === null ? "倍率未知" : formatMultiplier(value);
}

function formatMultiplier(value: number) {
  return `${formatDecimal(value, 6)}x`;
}

function formatTime(value: string | null) {
  if (!value) {
    return "未记录";
  }
  const date = parseTimestampLikeDate(value);
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

function formatDecimal(value: number, fractionDigits: number) {
  return formatTrimmedDecimal(value, fractionDigits);
}

const inputClassName =
  "h-8 min-w-0 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition placeholder:text-muted-foreground/70 focus:border-ring focus:ring-2 focus:ring-ring/30";

const tableScrollClassName = "overflow-x-auto border-y border-border";
const tableClassName = "min-w-[720px] w-full table-fixed text-left text-sm";
const tableHeaderClassName = "px-2.5 py-2 text-xs font-medium text-muted-foreground";
const tableCellClassName = "px-2.5 py-2.5 align-top text-sm text-foreground";
const updatedAtHeaderClassName = `${tableHeaderClassName} whitespace-nowrap`;
const updatedAtCellClassName = `${tableCellClassName} whitespace-nowrap text-muted-foreground`;
