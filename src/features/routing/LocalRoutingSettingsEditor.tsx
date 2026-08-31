import { useState, type KeyboardEvent } from "react";
import { RefreshCw, Save } from "lucide-react";
import { Button, SectionCard, SelectControl, StatusBadge, SwitchControl, useToast } from "@/components/ui";
import { groupCategoryDefinitions } from "@/lib/groupCategories";
import type { PricingGroupType, RoutingGroupFilter, RoutingPolicyConfigV3 } from "@/lib/types/routing";
import { collectorProxyModeLabels } from "@/lib/types/settings";
import { settingsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { routingProtectionStatusQueryOptions } from "@/lib/queries/routingQueries";
import { createDefaultRoutingPolicyConfig, routingPolicyConfigEqual, routingPolicyDraftFieldHints, useRoutingPolicyDraft } from "./useRoutingPolicyDraft";

type WeightKey = keyof Pick<RoutingPolicyConfigV3, "reliabilityWeight" | "responsivenessWeight" | "costWeight" | "preferenceWeight">;
type WeightPercentages = Record<WeightKey, number>;
type SourceWeightKey = "realTrafficPercent" | "monitoringPercent";

const WEIGHTS: Array<{ key: WeightKey; label: string }> = [
  { key: "reliabilityWeight", label: "可靠性" },
  { key: "responsivenessWeight", label: "响应速度" },
  { key: "costWeight", label: "成本" },
  { key: "preferenceWeight", label: "偏好" },
];

const inputClassName = "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface px-2.5 text-sm text-foreground outline-none transition-colors focus:border-ring focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:bg-surface-subtle disabled:text-muted-foreground";
const settingsBlockClassName = "grid gap-4 rounded-[var(--surface-radius)] bg-surface-subtle p-4";
const routingProxyModeLabels = {
  inherit: "继承全局设置",
  direct: "直连",
  system: "使用系统代理",
  manual: "手动代理地址",
} as const;
type RoutingProxyMode = keyof typeof routingProxyModeLabels;

const SCORE_PRESETS: Array<{ id: string; label: string; percentages: WeightPercentages }> = [
  { id: "balanced", label: "均衡", percentages: { reliabilityWeight: 40, responsivenessWeight: 25, costWeight: 20, preferenceWeight: 15 } },
  { id: "stable", label: "稳定优先", percentages: { reliabilityWeight: 50, responsivenessWeight: 25, costWeight: 15, preferenceWeight: 10 } },
  { id: "speed", label: "速度优先", percentages: { reliabilityWeight: 25, responsivenessWeight: 50, costWeight: 15, preferenceWeight: 10 } },
  { id: "cost", label: "成本优先", percentages: { reliabilityWeight: 25, responsivenessWeight: 20, costWeight: 45, preferenceWeight: 10 } },
];

const WEIGHT_TOTAL = 10_000;

function percentagesFromConfig(config: RoutingPolicyConfigV3): WeightPercentages {
  const total = WEIGHTS.reduce((sum, { key }) => sum + Math.max(0, config[key]), 0);
  if (total <= 0) {
    return { reliabilityWeight: 25, responsivenessWeight: 25, costWeight: 25, preferenceWeight: 25 };
  }
  return Object.fromEntries(WEIGHTS.map(({ key }) => [key, (Math.max(0, config[key]) / total) * 100])) as WeightPercentages;
}

function weightsFromPercentages(percentages: WeightPercentages): Pick<RoutingPolicyConfigV3, WeightKey> {
  const weights = Object.fromEntries(WEIGHTS.map(({ key }) => [key, Math.max(0, Math.round(percentages[key] * 100))])) as Pick<RoutingPolicyConfigV3, WeightKey>;
  const difference = WEIGHT_TOTAL - WEIGHTS.reduce((sum, { key }) => sum + weights[key], 0);
  const adjustmentKey = WEIGHTS[WEIGHTS.length - 1].key;
  weights[adjustmentKey] = Math.max(0, weights[adjustmentKey] + difference);
  return weights;
}

function normalizeChangedPercentage(current: WeightPercentages, key: WeightKey, value: number): WeightPercentages {
  const target = Math.min(100, Math.max(0, Number.isFinite(value) ? value : 0));
  const otherKeys = WEIGHTS.map(({ key: itemKey }) => itemKey).filter((itemKey) => itemKey !== key);
  const otherTotal = otherKeys.reduce((sum, itemKey) => sum + Math.max(0, current[itemKey]), 0);
  const remaining = 100 - target;
  if (otherTotal <= 0) {
    const share = remaining / otherKeys.length;
    return Object.fromEntries(WEIGHTS.map(({ key: itemKey }) => [itemKey, itemKey === key ? target : share])) as WeightPercentages;
  }
  return Object.fromEntries(WEIGHTS.map(({ key: itemKey }) => [
    itemKey,
    itemKey === key ? target : (Math.max(0, current[itemKey]) / otherTotal) * remaining,
  ])) as WeightPercentages;
}

function matchingPreset(percentages: WeightPercentages): string {
  const preset = SCORE_PRESETS.find(({ percentages: candidate }) =>
    WEIGHTS.every(({ key }) => Math.abs(candidate[key] - percentages[key]) < 0.05),
  );
  return preset?.id ?? "custom";
}

function SourceWeightControl({
  realTrafficPercent,
  monitoringPercent,
  editing,
  error,
  onEdit,
  onChange,
}: {
  realTrafficPercent: number;
  monitoringPercent: number;
  editing: SourceWeightKey | null;
  error: string | undefined;
  onEdit: (key: SourceWeightKey | null) => void;
  onChange: (key: SourceWeightKey, value: number) => void;
}) {
  const realValue = Number.isFinite(realTrafficPercent) ? realTrafficPercent : 0;
  const sliderValue = Math.min(100, Math.max(0, realValue));
  const realLabel = formatSourceWeight(realTrafficPercent);
  const monitoringLabel = formatSourceWeight(monitoringPercent);

  function finishEditing() {
    onEdit(null);
  }

  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      event.currentTarget.blur();
    }
  }

  function endpoint(key: SourceWeightKey, label: string, value: number, display: string) {
    if (editing === key) {
      return (
        <input
          autoFocus
          aria-label={`${label}权重（%）`}
          aria-invalid={Boolean(error)}
          className="h-7 w-16 rounded-[var(--surface-radius)] border border-ring bg-surface px-1.5 text-center text-sm font-normal tabular-nums text-foreground outline-none ring-2 ring-ring/20"
          type="number"
          min={0}
          max={100}
          step={1}
          value={value}
          onChange={(event) => onChange(key, Number(event.target.value))}
          onBlur={finishEditing}
          onKeyDown={handleKeyDown}
        />
      );
    }

    return (
      <button
        type="button"
        aria-label={`编辑${label}权重`}
        className="inline-flex h-7 min-w-16 items-center justify-center rounded-[var(--surface-radius)] border border-border bg-surface px-1.5 text-sm font-normal tabular-nums text-foreground shadow-surface outline-none transition hover:border-ring hover:bg-hover focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/30"
        onClick={() => onEdit(key)}
        title={`编辑${label}权重`}
      >
        {display}
      </button>
    );
  }

  return (
    <div className="grid gap-2" role="group" aria-label="质量来源权重">
      <div className="grid min-w-0 grid-cols-[minmax(0,auto)_minmax(4rem,1fr)_minmax(0,auto)] items-center gap-2 text-xs text-muted-foreground sm:gap-3">
        <span className="flex min-w-0 items-center gap-1 whitespace-nowrap">
          <span>真实流量（%）</span>
          {endpoint("realTrafficPercent", "真实流量", realTrafficPercent, realLabel)}
        </span>
        <div className="relative flex h-6 min-w-0 items-center">
          <div aria-hidden="true" className="pointer-events-none absolute inset-x-0 flex h-1.5 overflow-hidden rounded-full">
            <div className="h-full shrink-0 bg-primary-solid transition-[width]" style={{ width: `${sliderValue}%` }} />
            <div className="h-full min-w-0 flex-1 bg-border" />
          </div>
          <input
            aria-label="真实流量与监控权重比例滑块"
            aria-invalid={Boolean(error)}
            aria-valuetext={`真实流量 ${realLabel}%，监控 ${monitoringLabel}%`}
            className="relative z-10 h-6 w-full cursor-pointer bg-transparent accent-[var(--primary-solid)]"
            type="range"
            min={0}
            max={100}
            step={1}
            value={sliderValue}
            onChange={(event) => onChange("realTrafficPercent", Number(event.target.value))}
          />
        </div>
        <span className="flex min-w-0 items-center justify-end gap-1 whitespace-nowrap">
          <span>监控（%）</span>
          {endpoint("monitoringPercent", "监控", monitoringPercent, monitoringLabel)}
        </span>
      </div>
      {error ? <span className="font-normal text-danger-foreground" role="alert">{error}</span> : null}
    </div>
  );
}

function formatSourceWeight(value: number) {
  return Number.isFinite(value) ? String(value) : "0";
}

export function LocalRoutingSettingsEditor() {
  const [editingSourceWeight, setEditingSourceWeight] = useState<SourceWeightKey | null>(null);
  const toast = useToast();
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const protectionQuery = useActivityQuery(routingProtectionStatusQueryOptions());
  const { state: draft, setConfig, save, reload, discard, mergeRemote, overwriteRemote } = useRoutingPolicyDraft();
  const config = draft.config;
  const dirty = draft.status === "dirty" || draft.status === "conflict";
  const error = draft.error;
  const state = draft.status;
  const publication = policyPublicationFeedback(
    draft.publicationStatus,
    draft.publicationPollingState,
    draft.publicationError,
    draft.publicationFailureCode,
  );
  const fieldHints = routingPolicyDraftFieldHints(config);
  const fieldErrors = draft.fieldErrors;
  const sourceFieldError = (field: string) =>
    fieldErrors[`reliabilitySourceWeights.${field}`] ??
    fieldErrors.reliabilitySourceWeights ??
    fieldHints[`reliabilitySourceWeights.${field}`] ??
    fieldHints.reliabilitySourceWeights;
  const samplingFieldError = (field: string) =>
    fieldErrors[`reliabilitySampling.${field}`] ?? fieldErrors.reliabilitySampling;
  const retryFieldError = (field: string) =>
    fieldErrors[`retry.${field}`] ?? fieldErrors.retry;
  const circuitFieldError = (field: string) =>
    fieldErrors[`circuitBreaker.${field}`] ?? fieldErrors.circuitBreaker;
  const timeoutFieldError = (field: string) =>
    fieldErrors[`timeoutPolicy.${field}`] ?? fieldErrors.timeoutPolicy;
  const globalProxyMode = settingsQuery.data?.collectorProxyMode;
  const globalProxyLabel = globalProxyMode
    ? collectorProxyModeLabels[globalProxyMode]
    : "正在读取全局设置";
  const weightPercentages = config ? percentagesFromConfig(config) : null;
  const activePreset = weightPercentages ? matchingPreset(weightPercentages) : "custom";
  const currentProxyLabel = config && config.outboundProxyMode in routingProxyModeLabels
    ? routingProxyModeLabels[config.outboundProxyMode as RoutingProxyMode]
    : routingProxyModeLabels.inherit;

  function update<K extends keyof RoutingPolicyConfigV3>(key: K, value: RoutingPolicyConfigV3[K]) {
    if (!config) return;
    setConfig({ ...config, [key]: value });
  }

  function updateWeightPercentage(key: WeightKey, value: number) {
    if (!config) return;
    const nextPercentages = normalizeChangedPercentage(percentagesFromConfig(config), key, value);
    setConfig({ ...config, ...weightsFromPercentages(nextPercentages) });
  }

  function applyScorePreset(percentages: WeightPercentages) {
    if (!config) return;
    setConfig({ ...config, ...weightsFromPercentages(percentages) });
  }

  function updateSourceWeight(key: SourceWeightKey, value: number) {
    const target = Number.isFinite(value) ? value : 0;
    const otherKey = key === "realTrafficPercent" ? "monitoringPercent" : "realTrafficPercent";
    if (!config) return;
    update("reliabilitySourceWeights", {
      ...config.reliabilitySourceWeights,
      [key]: target,
      [otherKey]: 100 - target,
    });
  }

  function parseMaxRateMultiplier(value: string): number | null {
    const trimmed = value.trim();
    if (!trimmed) return null;
    const parsed = Number(trimmed);
    return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
  }

  function groupFilterValue(filter: RoutingGroupFilter): string {
    if (filter === "all_groups" || filter === "ungrouped_only") return filter;
    if ("group_type" in filter) return `group_type:${filter.group_type}`;
    return "all_groups";
  }

  function groupFilterFromValue(value: string): RoutingGroupFilter {
    if (value === "ungrouped_only") return "ungrouped_only";
    if (value.startsWith("group_type:")) {
      return { group_type: value.slice("group_type:".length) as PricingGroupType };
    }
    return "all_groups";
  }

  async function savePolicy() {
    const snapshot = await save();
    if (!snapshot) return;
    switch (snapshot.status) {
      case "staged":
        toast.info("路由策略已提交", "正在重建评分与熔断状态，完成切换后才会影响新请求。");
        break;
      case "ready":
        toast.info("路由策略已完成重建", "正在等待原子切换，当前运行策略暂未改变。");
        break;
      case "failed":
        toast.error("路由策略重建失败", "当前运行策略未改变，请查看诊断后重试。");
        break;
      case "active":
        toast.success("路由策略已生效");
        break;
      default:
        toast.info("路由策略状态已更新", `当前状态：${snapshot.status || "未知"}`);
    }
  }

  function restoreDefaults() {
    const defaults = createDefaultRoutingPolicyConfig();
    if (!config || routingPolicyConfigEqual(config, defaults)) return;
    setConfig(defaults);
  }

  if (!config) {
    return (
      <SectionCard title="策略配置">
        <div className="flex items-center justify-between gap-3 text-sm text-muted-foreground">
          <span>{state === "error" ? error : "正在加载后端策略..."}</span>
          <Button type="button" variant="secondary" size="sm" onClick={() => void reload()}>
            <RefreshCw className="size-4" />重新加载
          </Button>
        </div>
      </SectionCard>
    );
  }

  return (
    <fieldset
      aria-busy={state === "saving"}
      className="grid min-w-0 gap-3 border-0 p-0"
      disabled={state === "saving"}
    >
      <SectionCard title="策略配置">
        <div className="grid gap-3">
        <section className={settingsBlockClassName} aria-labelledby="routing-policy-boundaries-title">
          <div>
            <h3 id="routing-policy-boundaries-title" className="text-sm font-medium text-foreground">路由边界</h3>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>倍率上限（倍）</span>
              <div>
                <input
                  aria-label="倍率上限（倍）"
                  className={inputClassName}
                  type="number"
                  min={0}
                  step="any"
                  value={config.maxRateMultiplier ?? ""}
                  onChange={(event) => update("maxRateMultiplier", parseMaxRateMultiplier(event.target.value))}
                />
              </div>
            </label>
            <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
              <span>默认分组类型</span>
              <SelectControl
                ariaLabel="默认分组类型"
                className={inputClassName}
                value={groupFilterValue(config.routingGroupFilter ?? "all_groups")}
                options={[
                  { value: "all_groups", label: "全部分组" },
                  ...groupCategoryDefinitions.filter((definition) => definition.value !== "unknown").map((definition) => ({
                    value: `group_type:${definition.value}`,
                    label: definition.label,
                  })),
                  { value: "ungrouped_only", label: "仅未分组" },
                ]}
                onChange={(value) => update("routingGroupFilter", groupFilterFromValue(value))}
              />
            </label>
          </div>
        </section>
        <section
          className={settingsBlockClassName}
          aria-labelledby="routing-policy-weights-title"
          data-tour="routing-policy-profile"
        >
          <div>
            <h3 id="routing-policy-weights-title" className="text-sm font-medium text-foreground">评分偏好</h3>
          </div>
          <div className="flex flex-wrap gap-1" role="group" aria-label="评分策略预设">
            {SCORE_PRESETS.map((preset) => (
              <Button
                key={preset.id}
                type="button"
                variant={activePreset === preset.id ? "primary" : "outline"}
                size="sm"
                onClick={() => applyScorePreset(preset.percentages)}
              >
                {preset.label}
              </Button>
            ))}
            <Button type="button" variant={activePreset === "custom" ? "primary" : "outline"} size="sm" disabled>
              自定义
            </Button>
          </div>
          <div className="grid gap-2.5">
            {WEIGHTS.map(({ key, label }) => {
              const percentage = weightPercentages?.[key] ?? 0;
              return (
                <div key={key} className="grid grid-cols-[minmax(7rem,auto)_minmax(0,1fr)_auto] items-center gap-3 text-xs">
                  <span className="font-medium text-muted-foreground">{label}（%）</span>
                  <input
                    aria-label={`${label}（%）滑块`}
                    className="h-1.5 min-w-0 cursor-pointer accent-[var(--primary-solid)]"
                    type="range"
                    min={0}
                    max={100}
                    step={1}
                    value={Math.round(percentage)}
                    onChange={(event) => updateWeightPercentage(key, Number(event.target.value))}
                  />
                  <label className="flex items-center text-muted-foreground">
                    <input
                      aria-label={`${label}（%）`}
                      className={`${inputClassName} w-16 text-center tabular-nums`}
                      type="number"
                      min={0}
                      max={100}
                      step="1"
                      value={Number(percentage.toFixed(1))}
                      onChange={(event) => updateWeightPercentage(key, Number(event.target.value))}
                    />
                  </label>
                </div>
              );
            })}
          </div>
          <div className="border-t border-border pt-4">
            <h4 className="text-xs font-medium text-foreground">质量来源权重</h4>
          </div>
          <SourceWeightControl
            realTrafficPercent={config.reliabilitySourceWeights.realTrafficPercent}
            monitoringPercent={config.reliabilitySourceWeights.monitoringPercent}
            editing={editingSourceWeight}
            error={sourceFieldError("realTrafficPercent") ?? sourceFieldError("monitoringPercent")}
            onEdit={setEditingSourceWeight}
            onChange={updateSourceWeight}
          />
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>历史最小样本数</span>
              <input aria-label="历史最小样本数" aria-invalid={Boolean(samplingFieldError("historicalMinimumSamples"))} className={inputClassName} type="number" min={1} max={10_000} step={1} value={config.reliabilitySampling.historicalMinimumSamples} onChange={(event) => update("reliabilitySampling", { ...config.reliabilitySampling, historicalMinimumSamples: Number(event.target.value) })} />
              <span className="font-normal">历史窗口有效样本不足时，可靠性和响应速度使用乐观值。范围 1-10000，默认 15。</span>
              {samplingFieldError("historicalMinimumSamples") ? <span className="font-normal text-danger-foreground" role="alert">{samplingFieldError("historicalMinimumSamples")}</span> : null}
            </label>
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>最近最小样本数</span>
              <input aria-label="最近最小样本数" aria-invalid={Boolean(samplingFieldError("recentMinimumSamples"))} className={inputClassName} type="number" min={1} max={10_000} step={1} value={config.reliabilitySampling.recentMinimumSamples} onChange={(event) => update("reliabilitySampling", { ...config.reliabilitySampling, recentMinimumSamples: Number(event.target.value) })} />
              <span className="font-normal">最近 24 小时窗口有效样本不足时，可靠性和响应速度使用乐观值。范围 1-10000，默认 5。</span>
              {samplingFieldError("recentMinimumSamples") ? <span className="font-normal text-danger-foreground" role="alert">{samplingFieldError("recentMinimumSamples")}</span> : null}
            </label>
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>乐观可靠性（%）</span>
              <input aria-label="乐观可靠性（%）" aria-invalid={Boolean(samplingFieldError("optimisticReliabilityPercent"))} className={inputClassName} type="number" min={0} max={100} step={1} value={config.reliabilitySampling.optimisticReliabilityPercent} onChange={(event) => update("reliabilitySampling", { ...config.reliabilitySampling, optimisticReliabilityPercent: Number(event.target.value) })} />
              <span className="font-normal">仅作为样本不足时的排序假设，不写入真实统计。范围 0-100%，默认 95%。</span>
              {samplingFieldError("optimisticReliabilityPercent") ? <span className="font-normal text-danger-foreground" role="alert">{samplingFieldError("optimisticReliabilityPercent")}</span> : null}
            </label>
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>乐观响应时间（毫秒）</span>
              <input aria-label="乐观响应时间（毫秒）" aria-invalid={Boolean(samplingFieldError("optimisticLatencyMs"))} className={inputClassName} type="number" min={100} max={120_000} step={100} value={config.reliabilitySampling.optimisticLatencyMs} onChange={(event) => update("reliabilitySampling", { ...config.reliabilitySampling, optimisticLatencyMs: Number(event.target.value) })} />
              <span className="font-normal">仅作为样本不足时的排序假设，不写入真实统计。范围 100-120000 毫秒，默认 2500 毫秒（2.5 秒）。</span>
              {samplingFieldError("optimisticLatencyMs") ? <span className="font-normal text-danger-foreground" role="alert">{samplingFieldError("optimisticLatencyMs")}</span> : null}
            </label>
          </div>
        </section>

        <section className={settingsBlockClassName} aria-labelledby="routing-policy-timeouts-title">
          <div>
            <h3 id="routing-policy-timeouts-title" className="text-sm font-medium text-foreground">超时</h3>
          </div>
          {protectionQuery.isError || protectionQuery.data?.readModelStatus === "unavailable" ? (
            <p className="text-xs text-danger-foreground">运行时超时事实暂不可用；配置仍可编辑，保存值会用于后续新请求。</p>
          ) : null}
          <div className="grid gap-3 sm:grid-cols-2">
            {([
              ["connectSeconds", "连接超时", 1, 120, 10, null],
              ["firstByteSeconds", "首字节超时", 1, 300, 30, "连接建立后等待上游开始返回内容的最长时间；超过后视为上游响应异常。"],
              ["precommitSeconds", "提交前超时", 1, 600, 60, "输出提交给客户端前允许消耗的总预算，包含排队、重新规划和请求尝试。"],
              ["bufferedExecutionSeconds", "缓冲执行超时", 1, 1_800, 300, "非流式请求在完整响应返回前允许执行的最长时间。"],
              ["streamIdleSeconds", "流空闲超时", 1, 600, 90, "流式输出开始后两次输出之间允许的最长静默时间；触发后结束流，不自动重放已提交请求。"],
            ] as const).map(([key, label, min, max, defaultValue, description]) => {
              const error = timeoutFieldError(key);
              return (
                <label key={key} className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
                  <span>{label}（秒）</span>
                  <div>
                    <input
                      aria-label={`${label}（秒）`}
                      aria-invalid={Boolean(error)}
                      className={inputClassName}
                      type="number"
                      min={min}
                      max={max}
                      step={0.1}
                      value={config.timeoutPolicy[key]}
                      onChange={(event) => update("timeoutPolicy", { ...config.timeoutPolicy, [key]: Number(event.target.value) })}
                    />
                  </div>
                  <span className="font-normal">{description ? `${description} ` : null}范围 {min}-{max} 秒，默认 {defaultValue} 秒。</span>
                  {error ? <span className="font-normal text-danger-foreground" role="alert">{error}</span> : null}
                </label>
              );
            })}
          </div>
          {config.timeoutPolicy.precommitSeconds > config.timeoutPolicy.bufferedExecutionSeconds ? (
            <p className="text-xs text-warning-foreground" role="alert">提交前超时不能大于缓冲执行超时，保存时后端会拒绝该配置。</p>
          ) : null}
        </section>

        <section className={settingsBlockClassName} aria-labelledby="routing-policy-circuit-title">
          <div>
            <h3 id="routing-policy-circuit-title" className="text-sm font-medium text-foreground">熔断器设置</h3>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>恢复成功阈值（次）</span>
              <input aria-label="恢复成功阈值（次）" aria-invalid={Boolean(circuitFieldError("recoverySuccessThreshold"))} className={inputClassName} type="number" min={1} max={16} step={1} value={config.circuitBreaker.recoverySuccessThreshold} onChange={(event) => update("circuitBreaker", { ...config.circuitBreaker, recoverySuccessThreshold: Number(event.target.value) })} />
              <span className="font-normal">恢复阶段需要连续多少个独立真实请求成功才回到正常状态。范围 1-16 次，默认 2 次。</span>
              {circuitFieldError("recoverySuccessThreshold") ? <span className="font-normal text-danger-foreground" role="alert">{circuitFieldError("recoverySuccessThreshold")}</span> : null}
            </label>
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>恢复等待时间（秒）</span>
              <input aria-label="恢复等待时间（秒）" aria-invalid={Boolean(circuitFieldError("recoveryWaitSeconds"))} className={inputClassName} type="number" min={5} max={3_600} step={1} value={config.circuitBreaker.recoveryWaitSeconds} onChange={(event) => update("circuitBreaker", { ...config.circuitBreaker, recoveryWaitSeconds: Number(event.target.value) })} />
              <span className="font-normal">熔断后至少等待多久才有资格进行恢复尝试，反复失败会由系统自动延长。范围 5-3600 秒，默认 30 秒。</span>
              {circuitFieldError("recoveryWaitSeconds") ? <span className="font-normal text-danger-foreground" role="alert">{circuitFieldError("recoveryWaitSeconds")}</span> : null}
            </label>
          </div>
        </section>

        <section className={settingsBlockClassName} aria-labelledby="routing-policy-fallback-title">
          <div className="flex flex-wrap items-center justify-between gap-4">
            <div>
              <h3 id="routing-policy-fallback-title" className="text-sm font-medium text-foreground">应急回退</h3>
              <p className="mt-0.5 text-xs text-muted-foreground">候选余额耗尽时，仍允许其参与最后一次回退。</p>
            </div>
            <SwitchControl
              ariaLabel="允许耗尽余额作为应急回退"
              checked={config.allowDepletedFallback}
              showLabel={false}
              onCheckedChange={() => update("allowDepletedFallback", !config.allowDepletedFallback)}
            />
          </div>
        </section>

        <section className={settingsBlockClassName} aria-labelledby="routing-policy-affinity-title">
          <div className="flex flex-wrap items-center justify-between gap-4">
            <div>
              <h3 id="routing-policy-affinity-title" className="text-sm font-medium text-foreground">会话亲和</h3>
            </div>
            <SwitchControl
              ariaLabel="启用会话亲和"
              checked={config.affinityEnabled}
              showLabel={false}
              onCheckedChange={() => update("affinityEnabled", !config.affinityEnabled)}
            />
          </div>
          {config.affinityEnabled ? (
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>亲和时长（秒）</span>
              <div>
                <input aria-label="亲和时长（秒）" className={inputClassName} type="number" min={1} max={86400} value={config.affinityTtlSeconds} onChange={(event) => update("affinityTtlSeconds", Number(event.target.value))} />
              </div>
              <span className="font-normal">亲和记录保持有效的时间。范围 1-86400 秒，默认 300 秒。</span>
            </label>
          ) : null}
        </section>

        <section className={settingsBlockClassName} aria-labelledby="routing-policy-retry-title">
          <div>
            <h3 id="routing-policy-retry-title" className="text-sm font-medium text-foreground">重试设置</h3>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>最大重试次数（次）</span>
              <input aria-label="最大重试次数（次）" aria-invalid={Boolean(retryFieldError("maxRetryCount"))} className={inputClassName} type="number" min={0} max={3} step={1} value={config.retry.maxRetryCount} onChange={(event) => update("retry", { ...config.retry, maxRetryCount: Number(event.target.value) })} />
              <span className="font-normal">首把密钥之外最多再尝试多少把密钥。范围 0-3，默认 3。</span>
              {retryFieldError("maxRetryCount") ? <span className="font-normal text-danger-foreground" role="alert">{retryFieldError("maxRetryCount")}</span> : null}
            </label>
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>连续失败阈值（次）</span>
              <input aria-label="连续失败阈值（次）" aria-invalid={Boolean(retryFieldError("consecutiveFailureThreshold"))} className={inputClassName} type="number" min={1} max={10} step={1} value={config.retry.consecutiveFailureThreshold} onChange={(event) => update("retry", { ...config.retry, consecutiveFailureThreshold: Number(event.target.value) })} />
              <span className="font-normal">当前密钥失败后会继续重试；连续失败达到该次数后熔断并尝试下一把密钥。计数跨请求保留，范围 1-10 次，默认 3 次。</span>
              {retryFieldError("consecutiveFailureThreshold") ? <span className="font-normal text-danger-foreground" role="alert">{retryFieldError("consecutiveFailureThreshold")}</span> : null}
            </label>
          </div>
        </section>

        <footer
          className={`flex flex-wrap items-center gap-3 pt-4 ${error || dirty || publication ? "justify-between" : "justify-end"}`}
          data-tour="routing-policy-save"
        >
          {error || dirty || publication ? (
            <div className="flex min-h-5 min-w-0 flex-wrap items-center gap-2 text-xs" aria-live="polite">
              {error ? <p className="text-danger-foreground">{error}</p> : null}
              {dirty ? <StatusBadge tone="warning">未保存</StatusBadge> : null}
              {!dirty && publication ? (
                <>
                  <StatusBadge tone={publication.tone}>{publication.label}</StatusBadge>
                  <span className="min-w-0 text-muted-foreground">{publication.description}</span>
                </>
              ) : null}
            </div>
          ) : null}
          <div className="flex gap-2">
            <Button type="button" variant="secondary" size="sm" disabled={state === "saving"} onClick={restoreDefaults}>重置</Button>
            {state === "conflict" ? (
              <>
                <Button type="button" variant="secondary" size="sm" onClick={() => { discard(); void reload(); }}>重新加载</Button>
                <Button type="button" variant="secondary" size="sm" onClick={mergeRemote}>合并远端</Button>
                <Button type="button" variant="secondary" size="sm" onClick={overwriteRemote}>覆盖远端</Button>
              </>
            ) : null}
            <Button type="button" size="sm" disabled={!dirty || state === "saving"} onClick={() => void savePolicy()}><Save className="size-4" />保存</Button>
          </div>
        </footer>
        </div>
      </SectionCard>

      <SectionCard contentClassName="p-0" title="网络出口">
        <div className="grid min-h-14 items-start gap-2 px-3 py-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center sm:gap-4">
          <div className="min-w-0">
            <h2 id="routing-outbound-proxy-title" className="text-sm font-medium text-foreground">出站代理</h2>
            <p className="mt-0.5 break-words text-xs text-muted-foreground">本地路由向中转站发请求时使用的网络出口；站点单独设置仍优先于此项。</p>
          </div>
          <div className="min-w-0 w-full justify-self-stretch sm:w-auto sm:justify-self-end">
            <div className="grid gap-2">
              <SelectControl
                ariaLabel="本地路由出站代理"
                className={`${inputClassName} sm:w-auto`}
                value={(config.outboundProxyMode in routingProxyModeLabels
                  ? config.outboundProxyMode
                  : "inherit") as RoutingProxyMode}
                options={Object.entries(routingProxyModeLabels).map(([value, label]) => ({
                  value: value as RoutingProxyMode,
                  label: value === "inherit" ? `${label}（${globalProxyLabel}）` : label,
                }))}
                onChange={(value) => update("outboundProxyMode", value)}
              />
              {config.outboundProxyMode === "manual" ? (
                <input
                  aria-label="本地路由手动代理地址"
                  className={`${inputClassName} sm:w-[260px]`}
                  placeholder="http://127.0.0.1:7890"
                  value={config.outboundProxyUrl ?? ""}
                  onChange={(event) => update("outboundProxyUrl", event.target.value || null)}
                />
              ) : null}
              {config.outboundProxyMode !== "inherit" ? <p className="text-right text-xs text-muted-foreground">当前使用：{currentProxyLabel}</p> : null}
            </div>
          </div>
        </div>
      </SectionCard>
    </fieldset>
  );
}

function policyPublicationFeedback(
  status: string | null,
  pollingState: "idle" | "polling" | "unavailable" | "timed_out",
  publicationError: string | null,
  failureCode: string | null,
) {
  if (pollingState === "timed_out" || pollingState === "unavailable") {
    return {
      label: pollingState === "timed_out" ? "发布确认超时" : "发布状态不可用",
      description: publicationError ?? "尚未确认此策略已生效。",
      tone: "error" as const,
    };
  }
  switch (status) {
    case "staged":
      return {
        label: "等待重建",
        description: "策略已提交，但尚未影响运行中的请求。",
        tone: "info" as const,
      };
    case "ready":
      return {
        label: "等待切换",
        description: "重建已完成，正在等待原子切换。",
        tone: "warning" as const,
      };
    case "failed":
      return {
        label: "重建失败",
        description: publicationFailureDescription(failureCode),
        tone: "error" as const,
      };
    case "active":
      return null;
    case "expired":
      return {
        label: "发布已失效",
        description: "该 revision 或 generation 已被替代，尚未确认此策略生效。",
        tone: "warning" as const,
      };
    default:
      return status
        ? {
            label: "状态未知",
            description: `后端返回状态：${status}`,
            tone: "warning" as const,
          }
        : null;
  }
}

function publicationFailureDescription(failureCode: string | null): string {
  switch (failureCode) {
    case "generation_build_failed":
      return "评分或熔断状态重建失败，当前运行策略未改变。";
    case "generation_superseded":
      return "发布 generation 已被更新输入替代，当前运行策略未改变。";
    case "generation_qualification_failed":
      return "发布资格校验失败，当前运行策略未改变。";
    case "generation_cutover_failed":
      return "原子切换失败，当前运行策略未改变。";
    default:
      return "当前运行策略未改变。";
  }
}
