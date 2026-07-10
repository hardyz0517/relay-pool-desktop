import { useMemo, useState } from "react";
import { Coins, Image, RefreshCw, ShieldCheck, TrendingDown } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageActivation } from "@/components/shell/PageActivity";
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
import { listPricingRules } from "@/lib/api/economics";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { listStationKeys } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import { Sub2ApiPlatformIcon } from "@/features/stations/components/Sub2ApiPlatformIcon";
import { groupVisualMetaFor } from "@/features/stations/groupVisualMeta";
import { cn } from "@/lib/utils";
import type { PricingRule } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
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
  { value: "gpt", label: "GPT" },
  { value: "claude", label: "Claude" },
  { value: "gemini", label: "Gemini" },
  { value: "grok", label: "Grok" },
  { value: "image_generation", label: "生成图片" },
];

type PricingPageProps = {
  onOpenModelBasePrices: () => void;
};

export function PricingPage({ onOpenModelBasePrices }: PricingPageProps) {
  const toast = useToast();
  const [pricingRules, setPricingRules] = useState<PricingRule[]>([]);
  const [stations, setStations] = useState<Station[]>([]);
  const [stationKeys, setStationKeys] = useState<StationKey[]>([]);
  const [groupBindings, setGroupBindings] = useState<StationGroupBinding[]>([]);
  const [groupRates, setGroupRates] = useState<GroupRateRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [groupTypeFilter, setGroupTypeFilter] = useState<GroupTypeFilter>("all");
  const [query, setQuery] = useState("");
  const [selectedStationId, setSelectedStationId] = useState<string>("all");

  usePageActivation(({ isInitial }) => {
    void refresh(false, isInitial);
  });

  async function refresh(showSuccess = false, showLoading = true) {
    if (showLoading) {
      setLoading(true);
    }
    setError(null);
    try {
      const [nextPricingRules, nextStations] = await Promise.all([
        listPricingRules(),
        listStations(),
      ]);
      const [bindingLists, rateRecordLists, stationKeyLists] = await Promise.all([
        Promise.all(nextStations.map((station) => listStationGroupBindings(station.id))),
        Promise.all(nextStations.map((station) => listGroupRateRecords(station.id))),
        Promise.all(nextStations.map((station) => listStationKeys(station.id))),
      ]);

      setPricingRules(nextPricingRules);
      setStations(nextStations);
      setStationKeys(stationKeyLists.flat());
      setGroupBindings(bindingLists.flat());
      setGroupRates(rateRecordLists.flat());

      if (showSuccess) {
        toast.success("价格倍率已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新价格倍率失败", message);
    } finally {
      if (showLoading) {
        setLoading(false);
      }
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
      pricingRules,
      query,
      selectedStationId,
      stationKeys,
      stations,
    ],
  );

  return (
    <PageScaffold
      title="价格 / 倍率"
      actions={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <Button variant="secondary" onClick={onOpenModelBasePrices}>
            <Coins className="h-4 w-4" />
            模型基准价格
          </Button>
          <Button variant="secondary" onClick={() => void refresh(true)}>
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
              options={groupTypeFilterOptions}
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
          <div className="border-b border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">
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
              <GroupPricingSection key={section.groupType} section={section} />
            ))}
          </div>
        )}
      </SectionCard>
    </PageScaffold>
  );
}

function GroupPricingSection({ section }: { section: PricingGroupSection }) {
  return (
    <section className="grid gap-3 px-4 py-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          {section.groupType === "image_generation" && (
            <span className="inline-flex h-7 w-7 items-center justify-center rounded-md border border-slate-200 bg-white text-slate-600">
              <Image className="h-4 w-4" />
            </span>
          )}
          <h3 className="text-sm font-semibold text-slate-900">{section.title}</h3>
        </div>
        <div className="text-xs text-muted-foreground">{section.rows.length} 个分组</div>
      </div>

      <PricingRowsTable rows={section.rows} />
    </section>
  );
}

function PricingRowsTable({ rows }: { rows: PricingComparisonRow[] }) {
  return (
    <div className={tableScrollClassName}>
      <table className={tableClassName}>
        <colgroup>
          <col className="w-[22%]" />
          <col className="w-[28%]" />
          <col className="w-[20%]" />
          <col className="w-[14%]" />
          <col className="w-[16%]" />
        </colgroup>
        <thead>
          <tr className="border-b border-border">
            <th className={tableHeaderClassName}>中转站</th>
            <th className={tableHeaderClassName}>分组</th>
            <th className={tableHeaderClassName}>Key</th>
            <th className={tableHeaderClassName}>倍率</th>
            <th className={updatedAtHeaderClassName}>更新时间</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-border">
          {rows.map((row) => (
            <tr key={row.id} className={row.isCheapest ? cheapestRowClassName : undefined}>
              <td className={tableCellClassName}>
                <div className="font-medium text-slate-800">{row.stationName}</div>
              </td>
              <td className={tableCellClassName}>
                <PricingGroupBadge row={row} />
                {row.isCheapest && (
                  <div className="mt-0.5 text-xs font-medium text-emerald-700">当前最低</div>
                )}
              </td>
              <td className={`${tableCellClassName} truncate text-muted-foreground`}>
                {row.stationKeyName ?? "全站分组"}
              </td>
              <td className={`${tableCellClassName} tabular-nums font-semibold text-slate-800`}>
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
  const visualMeta = groupVisualMetaFor(row.groupName, row.groupRawJsonRedacted);

  return (
    <span
      className={cn(
        "inline-flex h-6 max-w-full items-center gap-1.5 rounded-md border px-2 text-xs font-semibold",
        visualMeta.badgeClassName,
      )}
      title={`${visualMeta.label} · ${row.groupName}`}
    >
      <Sub2ApiPlatformIcon platform={visualMeta.platform} className={visualMeta.iconClassName} />
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
  "h-8 w-[220px] rounded-[var(--surface-radius)] border border-slate-200 bg-white px-3 text-sm text-slate-800 outline-none transition placeholder:text-slate-400 focus:border-[hsl(var(--accent)/0.45)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.16)]";

const tableScrollClassName = "overflow-x-auto border-y border-border";
const tableClassName = "min-w-[720px] w-full table-fixed text-left text-sm";
const tableHeaderClassName = "px-2.5 py-2 text-xs font-medium text-muted-foreground";
const tableCellClassName = "px-2.5 py-2.5 align-top text-sm text-slate-700";
const updatedAtHeaderClassName = `${tableHeaderClassName} whitespace-nowrap`;
const updatedAtCellClassName = `${tableCellClassName} whitespace-nowrap text-muted-foreground`;
const cheapestRowClassName = "bg-emerald-50/70";
