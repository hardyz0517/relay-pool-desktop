import { useEffect, useMemo, useState } from "react";
import { RefreshCw, Save } from "lucide-react";
import { Button, SectionCard, SelectControl, SwitchControl, useToast } from "@/components/ui";
import { applyRoutingPolicyDocument, loadRoutingPolicy } from "@/lib/api/routing";
import { readError } from "@/lib/errors";
import { refreshRoutingQueries } from "@/lib/query/routingQuerySynchronization";
import { groupCategoryDefinitions } from "@/lib/groupCategories";
import type { PricingGroupType, RoutingGroupFilter, RoutingPolicyConfigV1 } from "@/lib/types/routing";
import { collectorProxyModeLabels } from "@/lib/types/settings";
import { settingsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { useQueryClient } from "@tanstack/react-query";

type SaveState = "idle" | "loading" | "dirty" | "saving" | "saved" | "error";

type WeightKey = keyof Pick<RoutingPolicyConfigV1, "reliabilityWeight" | "responsivenessWeight" | "costWeight" | "preferenceWeight">;
type WeightPercentages = Record<WeightKey, number>;

const WEIGHTS: Array<{ key: WeightKey; label: string }> = [
  { key: "reliabilityWeight", label: "可靠性" },
  { key: "responsivenessWeight", label: "响应速度" },
  { key: "costWeight", label: "成本" },
  { key: "preferenceWeight", label: "偏好" },
];

const inputClassName = "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface px-2.5 text-sm text-foreground outline-none transition-colors focus:border-ring focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:bg-surface-subtle disabled:text-muted-foreground";
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

function percentagesFromConfig(config: RoutingPolicyConfigV1): WeightPercentages {
  const total = WEIGHTS.reduce((sum, { key }) => sum + Math.max(0, config[key]), 0);
  if (total <= 0) {
    return { reliabilityWeight: 25, responsivenessWeight: 25, costWeight: 25, preferenceWeight: 25 };
  }
  return Object.fromEntries(WEIGHTS.map(({ key }) => [key, (Math.max(0, config[key]) / total) * 100])) as WeightPercentages;
}

function weightsFromPercentages(percentages: WeightPercentages): Pick<RoutingPolicyConfigV1, WeightKey> {
  const weights = Object.fromEntries(WEIGHTS.map(({ key }) => [key, Math.max(0, Math.round(percentages[key] * 100))])) as Pick<RoutingPolicyConfigV1, WeightKey>;
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

export function LocalRoutingSettingsEditor() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const [config, setConfig] = useState<RoutingPolicyConfigV1 | null>(null);
  const [saved, setSaved] = useState<RoutingPolicyConfigV1 | null>(null);
  const [revision, setRevision] = useState<number | null>(null);
  const [state, setState] = useState<SaveState>("loading");
  const [error, setError] = useState<string | null>(null);

  async function reload() {
    setState("loading");
    setError(null);
    try {
      const response = await loadRoutingPolicy();
      setConfig(response.config);
      setSaved(response.config);
      setRevision(response.revision);
      setState("idle");
    } catch (requestError) {
      setError(readError(requestError));
      setState("error");
    }
  }

  useEffect(() => { void reload(); }, []);

  const dirty = useMemo(
    () => JSON.stringify(config) !== JSON.stringify(saved),
    [config, saved],
  );
  const globalProxyMode = settingsQuery.data?.collectorProxyMode;
  const weightPercentages = config ? percentagesFromConfig(config) : null;
  const activePreset = weightPercentages ? matchingPreset(weightPercentages) : "custom";
  const currentProxyLabel = config && config.outboundProxyMode in routingProxyModeLabels
    ? routingProxyModeLabels[config.outboundProxyMode as RoutingProxyMode]
    : routingProxyModeLabels.inherit;

  function update<K extends keyof RoutingPolicyConfigV1>(key: K, value: RoutingPolicyConfigV1[K]) {
    setConfig((current) => current ? { ...current, [key]: value } : current);
    setState("dirty");
  }

  function updateWeightPercentage(key: WeightKey, value: number) {
    if (!config) return;
    const nextPercentages = normalizeChangedPercentage(percentagesFromConfig(config), key, value);
    setConfig((current) => current ? { ...current, ...weightsFromPercentages(nextPercentages) } : current);
    setState("dirty");
  }

  function applyScorePreset(percentages: WeightPercentages) {
    if (!config) return;
    setConfig((current) => current ? { ...current, ...weightsFromPercentages(percentages) } : current);
    setState("dirty");
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

  async function save() {
    if (!config || revision == null || state === "saving") return;
    setState("saving");
    setError(null);
    try {
      const response = await applyRoutingPolicyDocument({
        formatVersion: 1,
        baseRevision: revision,
        policy: config,
      });
      setConfig(response.config);
      setSaved(response.config);
      setRevision(response.revision);
      setState("saved");
      const synchronization = await refreshRoutingQueries(queryClient);
      if (synchronization.refreshed) toast.success("路由策略已保存");
      else toast.error("策略已保存，但路由状态刷新失败", readError(synchronization.errors[0]));
    } catch (requestError) {
      setState("error");
      setError(readError(requestError));
      toast.error("保存路由策略失败", readError(requestError));
    }
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
    <SectionCard title="策略配置">
      <div className="divide-y divide-border">
        <section className="grid gap-4 pb-5" aria-labelledby="routing-policy-boundaries-title">
          <div>
            <h3 id="routing-policy-boundaries-title" className="text-sm font-medium text-foreground">路由边界</h3>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>倍率上限</span>
              <div className="flex items-center gap-2">
                <input
                  aria-label="倍率上限"
                  className={inputClassName}
                  type="number"
                  min={0}
                  step="any"
                  value={config.maxRateMultiplier ?? ""}
                  onChange={(event) => update("maxRateMultiplier", parseMaxRateMultiplier(event.target.value))}
                />
                <span className="text-xs text-muted-foreground">x</span>
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
        <section className="grid gap-4 py-5" aria-labelledby="routing-policy-proxy-title">
          <div>
            <h3 id="routing-policy-proxy-title" className="text-sm font-medium text-foreground">出站代理</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">
              本地路由向中转站发请求时使用的网络出口；站点单独设置仍优先于此项。
            </p>
          </div>
          <div className="grid gap-3 sm:max-w-xl">
            <SelectControl
              ariaLabel="本地路由出站代理"
              className={inputClassName}
              value={(config.outboundProxyMode in routingProxyModeLabels
                ? config.outboundProxyMode
                : "inherit") as RoutingProxyMode}
              options={Object.entries(routingProxyModeLabels).map(([value, label]) => ({
                value: value as RoutingProxyMode,
                label,
              }))}
              onChange={(value) => update("outboundProxyMode", value)}
            />
            {config.outboundProxyMode === "manual" ? (
              <input
                aria-label="本地路由手动代理地址"
                className={inputClassName}
                placeholder="http://127.0.0.1:7890"
                value={config.outboundProxyUrl ?? ""}
                onChange={(event) => update("outboundProxyUrl", event.target.value || null)}
              />
            ) : null}
            {config.outboundProxyMode === "inherit" ? (
              <p className="text-xs text-muted-foreground">
                当前使用：{globalProxyMode ? collectorProxyModeLabels[globalProxyMode] : "正在读取全局设置"}
              </p>
            ) : <p className="text-xs text-muted-foreground">当前使用：{currentProxyLabel}</p>}
          </div>
        </section>
        <section className="grid gap-4 py-5" aria-labelledby="routing-policy-weights-title">
          <div>
            <h3 id="routing-policy-weights-title" className="text-sm font-medium text-foreground">评分偏好</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">调整各项因素的重要程度，其他比例会自动归一化。</p>
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
                <div key={key} className="grid grid-cols-[5rem_minmax(0,1fr)_auto] items-center gap-3 text-xs">
                  <span className="font-medium text-muted-foreground">{label}</span>
                  <input
                    aria-label={`${label}滑块`}
                    className="h-1.5 min-w-0 cursor-pointer accent-[var(--primary-solid)]"
                    type="range"
                    min={0}
                    max={100}
                    step={1}
                    value={Math.round(percentage)}
                    onChange={(event) => updateWeightPercentage(key, Number(event.target.value))}
                  />
                  <label className="flex items-center gap-1 text-muted-foreground">
                    <input
                      aria-label={`${label}百分比`}
                      className={`${inputClassName} w-16 text-right tabular-nums`}
                      type="number"
                      min={0}
                      max={100}
                      step="1"
                      value={Number(percentage.toFixed(1))}
                      onChange={(event) => updateWeightPercentage(key, Number(event.target.value))}
                    />
                    <span>%</span>
                  </label>
                </div>
              );
            })}
          </div>
        </section>

        <section className="grid gap-4 py-5" aria-labelledby="routing-policy-runtime-title">
          <div>
            <h3 id="routing-policy-runtime-title" className="text-sm font-medium text-foreground">候选与探索</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">控制每次请求的候选范围与探索额度。</p>
          </div>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>最大候选数</span>
              <input className={inputClassName} type="number" min={1} max={1024} value={config.maxCandidates} onChange={(event) => update("maxCandidates", Number(event.target.value))} />
            </label>
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>探索比例</span>
              <div className="flex items-center gap-2">
                <input aria-label="探索比例" className={inputClassName} type="number" min={0} max={100} step="0.01" value={config.explorationShareBasisPoints / 100} onChange={(event) => update("explorationShareBasisPoints", Math.min(10_000, Math.max(0, Math.round(Number(event.target.value) * 100) || 0)))} />
                <span className="text-xs text-muted-foreground">%</span>
              </div>
              <span className="text-xs font-normal text-muted-foreground">控制每次请求用于探索候选的概率。</span>
            </label>
          </div>
        </section>

        <section className="grid gap-4 py-5" aria-labelledby="routing-policy-fallback-title">
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

        <section className="grid gap-4 py-5" aria-labelledby="routing-policy-affinity-title">
          <div className="flex flex-wrap items-center justify-between gap-4">
            <div>
              <h3 id="routing-policy-affinity-title" className="text-sm font-medium text-foreground">会话亲和</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">在有效期内优先复用同一候选密钥。</p>
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
              <span>亲和时长</span>
              <div className="flex items-center gap-2">
                <input aria-label="亲和时长" className={inputClassName} type="number" min={1} max={86400} value={config.affinityTtlSeconds} onChange={(event) => update("affinityTtlSeconds", Number(event.target.value))} />
                <span className="text-xs text-muted-foreground">秒</span>
              </div>
            </label>
          ) : null}
        </section>

        <footer className="flex flex-wrap items-center justify-between gap-3 pt-4">
          <div className="min-h-5 text-xs" aria-live="polite">
            {error ? <p className="text-danger-foreground">{error}</p> : dirty ? <span className="text-muted-foreground">存在未保存的修改</span> : null}
          </div>
          <div className="flex gap-2">
            <Button type="button" variant="secondary" size="sm" disabled={!dirty || state === "saving"} onClick={() => { setConfig(saved); setState("idle"); }}><RefreshCw className="size-4" />撤销</Button>
            <Button type="button" size="sm" disabled={!dirty || state === "saving"} onClick={() => void save()}><Save className="size-4" />保存策略</Button>
          </div>
        </footer>
      </div>
    </SectionCard>
  );
}
