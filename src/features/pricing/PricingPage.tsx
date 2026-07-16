import { useMemo, useState } from "react";
import { Coins, Image, RefreshCw, ShieldCheck, TrendingDown } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageRefreshEnabled } from "@/components/shell/PageActivity";
import {
  Button,
  EmptyState,
  MetricCard,
  SegmentedControl,
  SectionCard,
  SelectControl,
  Toolbar,
  useToast,
} from "@/components/ui";
import { readError } from "@/lib/errors";
import { formatTrimmedDecimal } from "@/lib/formatters";
import { parseTimestampLikeDate } from "@/lib/time";
import { openStationWebsite } from "@/lib/api/stations";
import { Sub2ApiPlatformIcon } from "@/features/stations/components/Sub2ApiPlatformIcon";
import { groupVisualMetaFor } from "@/features/stations/groupVisualMeta";
import { groupVisualClassNames } from "@/features/stations/groupVisualStyles";
import { groupCategoryDefinitions } from "@/lib/groupCategories";
import { pricingComparisonQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { cn } from "@/lib/utils";
import {
  buildPricingComparisonViewModel,
  type PricingComparisonRow,
  type PricingComparisonViewModel,
  type PricingGroupSection,
  type PricingGroupType,
} from "./pricingComparisonViewModel";

type GroupTypeFilter = PricingGroupType | "all";
type EmptyReason = PricingComparisonViewModel["emptyReason"];

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
};

export function PricingPage({ onOpenModelBasePrices }: PricingPageProps) {
  const toast = useToast();
  const refreshEnabled = usePageRefreshEnabled();
  const pricingQuery = useActivityQuery(
    refreshEnabled,
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

  async function refresh(showSuccess = false) {
    try {
      await pricingQuery.refetch({ throwOnError: true });
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
        },
      }),
    [
      groupBindings,
      groupRates,
      groupTypeFilter,
      developerModeEnabled,
      pricingRules,
      query,
      selectedStationId,
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

  return (
    <PageScaffold
      title="价格 / 倍率"
      actions={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <Button variant="secondary" onClick={onOpenModelBasePrices}>
            <Coins className="h-4 w-4" />
            模型基准价格
          </Button>
          <Button variant="secondary" disabled={pricingQuery.isFetching} onClick={() => void refresh(true)}>
            <RefreshCw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      <div className="grid gap-[var(--shell-page-gap)] md:grid-cols-2">
        <MetricCard
          icon={ShieldCheck}
          label="可比分组"
          value={`${viewModel.metrics.comparableGroupCount}`}
          detail="已采集并可折算的分组倍率"
        />
        <MetricCard
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
        contentClassName="overflow-visible rounded-none border-0 bg-transparent p-0 shadow-none"
      >
        <Toolbar className="items-start border-x-0 border-t-0 bg-transparent px-0">
          <div className="flex w-full flex-wrap items-center gap-2">
            <SegmentedControl
              ariaLabel="按分组类型筛选"
              value={groupTypeFilter}
              options={visibleGroupTypeFilterOptions(developerModeEnabled)}
              onChange={setGroupTypeFilter}
              className="w-full max-w-[560px] sm:w-auto"
            />
            <label className="sr-only" htmlFor="pricing-group-search">
              搜索中转站 / Key / 分组
            </label>
            <input
              id="pricing-group-search"
              className={inputClassName}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索中转站 / Key / 分组"
            />
            <SelectControl
              ariaLabel="按中转站筛选"
              className="w-[180px]"
              value={selectedStationId}
              options={[
                { value: "all", label: "全部中转站" },
                ...stations.map((station) => ({ value: station.id, label: station.name })),
              ]}
              onChange={setSelectedStationId}
            />
          </div>
        </Toolbar>

        {error && (
          <div className="border-b border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
            {error}
          </div>
        )}

        {loading ? (
          <div className="px-4 py-5 text-sm text-muted-foreground">正在读取分组倍率...</div>
        ) : viewModel.sections.length === 0 ? (
          <div className="p-4">
            <PricingEmptyState reason={viewModel.emptyReason} />
          </div>
        ) : (
          <div className="divide-y divide-border">
            {viewModel.sections.map((section) => (
              <GroupPricingSection
                key={section.groupType}
                section={section}
                onOpenStation={handleOpenStation}
              />
            ))}
          </div>
        )}
      </SectionCard>
    </PageScaffold>
  );
}

function GroupPricingSection({
  section,
  onOpenStation,
}: {
  section: PricingGroupSection;
  onOpenStation: (stationId: string, stationName: string) => void;
}) {
  return (
    <section className="grid gap-3 px-4 py-4">
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

      <PricingRowsTable rows={section.rows} onOpenStation={onOpenStation} />
    </section>
  );
}

function PricingRowsTable({
  rows,
  onOpenStation,
}: {
  rows: PricingComparisonRow[];
  onOpenStation: (stationId: string, stationName: string) => void;
}) {
  return (
    <div className={tableScrollClassName}>
      <table className={tableClassName}>
        <colgroup>
          <col className="w-[28%]" />
          <col className="w-[38%]" />
          <col className="w-[16%]" />
          <col className="w-[18%]" />
        </colgroup>
        <thead>
          <tr className="border-b border-border">
            <th className={tableHeaderClassName}>中转站</th>
            <th className={tableHeaderClassName}>分组</th>
            <th className={tableHeaderClassName}>倍率</th>
            <th className={updatedAtHeaderClassName}>更新时间</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {rows.map((row) => (
            <tr key={row.id} className={row.isCheapest ? cheapestRowClassName : undefined}>
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
                {row.isCheapest && (
                  <div className="mt-0.5 text-xs font-medium text-success-foreground">当前最低</div>
                )}
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
  "h-8 w-[220px] rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition placeholder:text-muted-foreground/70 focus:border-ring focus:ring-2 focus:ring-ring/30";

const tableScrollClassName = "overflow-x-auto border-y border-border";
const tableClassName = "min-w-[720px] w-full table-fixed text-left text-sm";
const tableHeaderClassName = "px-2.5 py-2 text-xs font-medium text-muted-foreground";
const tableCellClassName = "px-2.5 py-2.5 align-top text-sm text-foreground";
const updatedAtHeaderClassName = `${tableHeaderClassName} whitespace-nowrap`;
const updatedAtCellClassName = `${tableCellClassName} whitespace-nowrap text-muted-foreground`;
const cheapestRowClassName = "bg-success-surface";
