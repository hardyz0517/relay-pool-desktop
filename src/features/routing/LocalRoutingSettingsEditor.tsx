import { RefreshCw, Save } from "lucide-react";
import { Button, SectionCard, SelectControl, SwitchControl, useToast } from "@/components/ui";
import { groupCategoryDefinitions } from "@/lib/groupCategories";
import type { PricingGroupType, RoutingGroupFilter, RoutingPolicyConfigV2 } from "@/lib/types/routing";
import { collectorProxyModeLabels } from "@/lib/types/settings";
import { settingsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { routingProtectionStatusQueryOptions } from "@/lib/queries/routingQueries";
import { createDefaultRoutingPolicyConfig, routingPolicyConfigEqual, routingPolicyDraftFieldHints, useRoutingPolicyDraft } from "./useRoutingPolicyDraft";

type WeightKey = keyof Pick<RoutingPolicyConfigV2, "reliabilityWeight" | "responsivenessWeight" | "costWeight" | "preferenceWeight">;
type WeightPercentages = Record<WeightKey, number>;

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

function percentagesFromConfig(config: RoutingPolicyConfigV2): WeightPercentages {
  const total = WEIGHTS.reduce((sum, { key }) => sum + Math.max(0, config[key]), 0);
  if (total <= 0) {
    return { reliabilityWeight: 25, responsivenessWeight: 25, costWeight: 25, preferenceWeight: 25 };
  }
  return Object.fromEntries(WEIGHTS.map(({ key }) => [key, (Math.max(0, config[key]) / total) * 100])) as WeightPercentages;
}

function weightsFromPercentages(percentages: WeightPercentages): Pick<RoutingPolicyConfigV2, WeightKey> {
  const weights = Object.fromEntries(WEIGHTS.map(({ key }) => [key, Math.max(0, Math.round(percentages[key] * 100))])) as Pick<RoutingPolicyConfigV2, WeightKey>;
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
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const protectionQuery = useActivityQuery(routingProtectionStatusQueryOptions());
  const { state: draft, setConfig, save, reload, discard, mergeRemote, overwriteRemote } = useRoutingPolicyDraft();
  const config = draft.config;
  const dirty = draft.status === "dirty" || draft.status === "conflict";
  const error = draft.error;
  const state = draft.status;
  const fieldHints = routingPolicyDraftFieldHints(config);
  const fieldErrors = draft.fieldErrors;
  const protectionFieldError = (field: string) =>
    fieldErrors[`protectionProfile.${field}`] ?? fieldErrors.protectionProfile;
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

  function update<K extends keyof RoutingPolicyConfigV2>(key: K, value: RoutingPolicyConfigV2[K]) {
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
    const didSave = await save();
    if (didSave) toast.success("路由策略已保存");
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
    <div className="grid gap-3">
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
        </section>

        <section className={settingsBlockClassName} aria-labelledby="routing-policy-runtime-title">
          <div>
            <h3 id="routing-policy-runtime-title" className="text-sm font-medium text-foreground">候选与探索</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">控制每次请求的候选范围与探索额度。</p>
          </div>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>最大候选数</span>
              <input aria-label="最大候选数" className={inputClassName} type="number" min={1} max={1024} value={config.maxCandidates} onChange={(event) => update("maxCandidates", Number(event.target.value))} />
            </label>
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>探索比例（%）</span>
              <div>
                <input aria-label="探索比例（%）" className={inputClassName} type="number" min={0} max={100} step="0.01" value={config.explorationShareBasisPoints / 100} onChange={(event) => update("explorationShareBasisPoints", Math.min(10_000, Math.max(0, Math.round(Number(event.target.value) * 100) || 0)))} />
              </div>
              <span className="text-xs font-normal text-muted-foreground">控制每次请求用于探索候选的概率。</span>
            </label>
          </div>
        </section>

        <section className={settingsBlockClassName} aria-labelledby="routing-policy-timeouts-title">
          <div>
            <h3 id="routing-policy-timeouts-title" className="text-sm font-medium text-foreground">超时</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">控制新启动的本地路由实例；保存后需要重启本地路由才会替换当前运行时限制。</p>
          </div>
          {protectionQuery.isError || protectionQuery.data?.readModelStatus === "unavailable" ? (
            <p className="text-xs text-danger-foreground">运行时超时事实暂不可用；配置仍可编辑，重启后按保存值生效。</p>
          ) : null}
          <div className="grid gap-3 sm:grid-cols-2">
            {([
              ["connectSeconds", "连接超时", 1, 120],
              ["firstByteSeconds", "首字节超时", 1, 300],
              ["precommitSeconds", "提交前超时", 1, 600],
              ["bufferedExecutionSeconds", "缓冲执行超时", 1, 1_800],
              ["streamIdleSeconds", "流空闲超时", 1, 600],
            ] as const).map(([key, label, min, max]) => {
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
                  {error ? <span className="font-normal text-danger-foreground" role="alert">{error}</span> : null}
                </label>
              );
            })}
          </div>
          {config.timeoutPolicy.precommitSeconds > config.timeoutPolicy.bufferedExecutionSeconds ? (
            <p className="text-xs text-warning-foreground" role="alert">提交前超时不能大于缓冲执行超时，保存时后端会拒绝该配置。</p>
          ) : null}
        </section>

        {config ? (
            <section className={settingsBlockClassName} aria-labelledby="routing-policy-protection-profile-title">
              <div>
                <h3 id="routing-policy-protection-profile-title" className="text-sm font-medium text-foreground">错误率保护参数</h3>
                <p className="mt-0.5 text-xs text-muted-foreground">默认关闭；开启后只影响跨请求健康保护，不改变单次请求的重试安全门。</p>
              </div>
              <div className="flex items-center justify-between gap-4 text-xs">
                <span className="font-medium text-muted-foreground">启用错误率保护</span>
                <SwitchControl
                  ariaLabel="启用错误率保护"
                  checked={config.protectionProfile.enabled}
                  showLabel={false}
                  onCheckedChange={() => update("protectionProfile", { ...config.protectionProfile, enabled: !config.protectionProfile.enabled })}
                />
              </div>
              {protectionFieldError("enabled") ? <p className="text-xs text-danger-foreground" role="alert">{protectionFieldError("enabled")}</p> : null}
              <div className="grid gap-3 sm:grid-cols-2">
                <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
                  <span>统计窗口样本数</span>
                  <input aria-label="统计窗口样本数" aria-invalid={Boolean(protectionFieldError("windowMaxSamples"))} aria-describedby={protectionFieldError("windowMaxSamples") ? "routing-error-protection-window-max-samples" : undefined} className={inputClassName} type="number" min={1} max={256} step={1} value={config.protectionProfile.windowMaxSamples} onChange={(event) => update("protectionProfile", { ...config.protectionProfile, windowMaxSamples: Number(event.target.value) })} />
                  {protectionFieldError("windowMaxSamples") ? <span id="routing-error-protection-window-max-samples" className="font-normal text-danger-foreground" role="alert">{protectionFieldError("windowMaxSamples")}</span> : null}
                </label>
                <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
                  <span>统计窗口时长（秒）</span>
                  <div><input aria-label="统计窗口时长（秒）" aria-invalid={Boolean(protectionFieldError("windowSeconds"))} aria-describedby={protectionFieldError("windowSeconds") ? "routing-error-protection-window-seconds" : undefined} className={inputClassName} type="number" min={1} max={86_400} step={0.1} value={config.protectionProfile.windowSeconds} onChange={(event) => update("protectionProfile", { ...config.protectionProfile, windowSeconds: Number(event.target.value) })} /></div>
                  {protectionFieldError("windowSeconds") ? <span id="routing-error-protection-window-seconds" className="font-normal text-danger-foreground" role="alert">{protectionFieldError("windowSeconds")}</span> : null}
                </label>
                <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
                  <span>最小样本数</span>
                  <input aria-label="最小样本数" aria-invalid={Boolean(protectionFieldError("minSamples"))} aria-describedby={protectionFieldError("minSamples") ? "routing-error-protection-min-samples" : undefined} className={inputClassName} type="number" min={1} max={256} step={1} value={config.protectionProfile.minSamples} onChange={(event) => update("protectionProfile", { ...config.protectionProfile, minSamples: Number(event.target.value) })} />
                  {protectionFieldError("minSamples") ? <span id="routing-error-protection-min-samples" className="font-normal text-danger-foreground" role="alert">{protectionFieldError("minSamples")}</span> : null}
                </label>
                <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
                  <span>失败率阈值（%）</span>
                  <div><input aria-label="失败率阈值（%）" aria-invalid={Boolean(protectionFieldError("failureThresholdPercent"))} aria-describedby={protectionFieldError("failureThresholdPercent") ? "routing-error-protection-failure-threshold" : undefined} className={inputClassName} type="number" min={1} max={100} step={1} value={config.protectionProfile.failureThresholdPercent} onChange={(event) => update("protectionProfile", { ...config.protectionProfile, failureThresholdPercent: Number(event.target.value) })} /></div>
                  {protectionFieldError("failureThresholdPercent") ? <span id="routing-error-protection-failure-threshold" className="font-normal text-danger-foreground" role="alert">{protectionFieldError("failureThresholdPercent")}</span> : null}
                </label>
                <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
                  <span>半开成功次数</span>
                  <input aria-label="半开成功次数" aria-invalid={Boolean(protectionFieldError("halfOpenSuccessesToClose"))} aria-describedby={protectionFieldError("halfOpenSuccessesToClose") ? "routing-error-protection-half-open-successes" : undefined} className={inputClassName} type="number" min={1} max={16} step={1} value={config.protectionProfile.halfOpenSuccessesToClose} onChange={(event) => update("protectionProfile", { ...config.protectionProfile, halfOpenSuccessesToClose: Number(event.target.value) })} />
                  {protectionFieldError("halfOpenSuccessesToClose") ? <span id="routing-error-protection-half-open-successes" className="font-normal text-danger-foreground" role="alert">{protectionFieldError("halfOpenSuccessesToClose")}</span> : null}
                </label>
              </div>
            </section>
          ) : null}

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
              <span>亲和时长（秒）</span>
              <div>
                <input aria-label="亲和时长（秒）" className={inputClassName} type="number" min={1} max={86400} value={config.affinityTtlSeconds} onChange={(event) => update("affinityTtlSeconds", Number(event.target.value))} />
              </div>
            </label>
          ) : null}
        </section>

        <section className={settingsBlockClassName} aria-labelledby="routing-policy-retry-title">
          <div>
            <h3 id="routing-policy-retry-title" className="text-sm font-medium text-foreground">重试与故障转移</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">只控制可安全重放的容量类失败；认证、请求参数、已提交响应等错误不会被设置强行重试。仅影响保存后创建的请求。</p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>单个请求最大尝试次数</span>
                <input aria-label="单个请求最大尝试次数" aria-invalid={Boolean(fieldErrors["retryFailover.maxTotalAttempts"])} aria-describedby={fieldErrors["retryFailover.maxTotalAttempts"] ? "routing-error-max-total-attempts" : undefined} className={inputClassName} type="number" min={1} max={4} step={1} value={config.retryFailover.maxTotalAttempts} onChange={(event) => update("retryFailover", { ...config.retryFailover, maxTotalAttempts: Number(event.target.value) })} />
                <span className="font-normal">包含第一次发送，范围 1-4。</span>
                {fieldErrors["retryFailover.maxTotalAttempts"] ? <span id="routing-error-max-total-attempts" className="font-normal text-danger-foreground" role="alert">{fieldErrors["retryFailover.maxTotalAttempts"]}</span> : null}
            </label>
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>同目标容量重试次数</span>
                <input aria-label="同目标容量重试次数" aria-invalid={Boolean(fieldErrors["retryFailover.maxSameTargetCapacityRetries"])} aria-describedby={fieldErrors["retryFailover.maxSameTargetCapacityRetries"] ? "routing-error-same-target-retries" : undefined} className={inputClassName} type="number" min={0} max={2} step={1} value={config.retryFailover.maxSameTargetCapacityRetries} onChange={(event) => update("retryFailover", { ...config.retryFailover, maxSameTargetCapacityRetries: Number(event.target.value) })} />
              <span className="font-normal">范围 0-2，且必须小于最大尝试次数。</span>
              {fieldHints["retryFailover.maxSameTargetCapacityRetries"] ? <span className="font-normal text-warning-foreground" role="alert">{fieldHints["retryFailover.maxSameTargetCapacityRetries"]}</span> : null}
                {fieldErrors["retryFailover.maxSameTargetCapacityRetries"] ? <span id="routing-error-same-target-retries" className="font-normal text-danger-foreground" role="alert">{fieldErrors["retryFailover.maxSameTargetCapacityRetries"]}</span> : null}
            </label>
            <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
              <span>容量重试总等待预算（秒）</span>
              <div>
                <input aria-label="容量重试总等待预算（秒）" aria-invalid={Boolean(fieldErrors["retryFailover.capacityRetryWaitBudgetSeconds"])} aria-describedby={fieldErrors["retryFailover.capacityRetryWaitBudgetSeconds"] ? "routing-error-wait-budget" : undefined} className={inputClassName} type="number" min={0} max={2} step={0.05} value={config.retryFailover.capacityRetryWaitBudgetSeconds} onChange={(event) => update("retryFailover", { ...config.retryFailover, capacityRetryWaitBudgetSeconds: Number(event.target.value) })} />
              </div>
              <span className="font-normal">范围 0-2，所有等待共享此预算。</span>
              {fieldErrors["retryFailover.capacityRetryWaitBudgetSeconds"] ? <span id="routing-error-wait-budget" className="font-normal text-danger-foreground" role="alert">{fieldErrors["retryFailover.capacityRetryWaitBudgetSeconds"]}</span> : null}
            </label>
            <div className="flex max-w-xl items-start justify-between gap-4 rounded-[var(--surface-radius)] border border-border bg-surface-subtle p-3 text-xs">
              <div>
                <p className="font-medium text-foreground">允许跨容量域回退</p>
                <p className="mt-0.5 font-normal text-muted-foreground">同一容量域不可用时，允许规划器尝试其他容量域。</p>
              </div>
              <SwitchControl ariaLabel="允许跨容量域回退" checked={config.retryFailover.allowCrossCapacityDomainFallback} showLabel={false} onCheckedChange={() => update("retryFailover", { ...config.retryFailover, allowCrossCapacityDomainFallback: !config.retryFailover.allowCrossCapacityDomainFallback })} />
            </div>
          </div>
        </section>

        <footer
          className={`flex flex-wrap items-center gap-3 pt-4 ${error || dirty ? "justify-between" : "justify-end"}`}
          data-tour="routing-policy-save"
        >
          {error || dirty ? (
            <div className="min-h-5 text-xs" aria-live="polite">
              {error ? <p className="text-danger-foreground">{error}</p> : <span className="text-muted-foreground">存在未保存的修改</span>}
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
    </div>
  );
}
