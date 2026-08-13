import { useEffect, useMemo, useState } from "react";
import { RefreshCw, Save } from "lucide-react";
import { Button, SectionCard, SelectControl, StatusBadge, SwitchControl, useToast } from "@/components/ui";
import { loadRoutingPolicy, updateRoutingPolicy } from "@/lib/api/routing";
import { readError } from "@/lib/errors";
import { refreshRoutingQueries } from "@/lib/query/routingQuerySynchronization";
import { groupCategoryDefinitions } from "@/lib/groupCategories";
import type { PricingGroupType, RoutingGroupFilter, RoutingPolicyConfigV1 } from "@/lib/types/routing";
import { useQueryClient } from "@tanstack/react-query";

type SaveState = "idle" | "loading" | "dirty" | "saving" | "saved" | "error";

const WEIGHTS: Array<{ key: keyof Pick<RoutingPolicyConfigV1, "reliabilityWeight" | "responsivenessWeight" | "costWeight" | "preferenceWeight">; label: string }> = [
  { key: "reliabilityWeight", label: "可靠性" },
  { key: "responsivenessWeight", label: "响应速度" },
  { key: "costWeight", label: "成本" },
  { key: "preferenceWeight", label: "偏好" },
];

const inputClassName = "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface px-2.5 text-sm text-foreground outline-none transition-colors focus:border-ring focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:bg-surface-subtle disabled:text-muted-foreground";
const weightInputClassName = `${inputClassName} h-10 text-base font-semibold tabular-nums`;

export function LocalRoutingSettingsEditor() {
  const toast = useToast();
  const queryClient = useQueryClient();
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
  const total = config
    ? config.reliabilityWeight + config.responsivenessWeight + config.costWeight + config.preferenceWeight
    : 0;

  function update<K extends keyof RoutingPolicyConfigV1>(key: K, value: RoutingPolicyConfigV1[K]) {
    setConfig((current) => current ? { ...current, [key]: value } : current);
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
    if (!config || revision == null || total !== 10_000 || state === "saving") return;
    setState("saving");
    setError(null);
    try {
      const response = await updateRoutingPolicy({ config, expectedRevision: revision });
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
      <SectionCard title="智能路由策略">
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
    <SectionCard title="智能路由策略">
      <div className="divide-y divide-border">
        <section className="grid gap-4 pb-5" aria-labelledby="routing-policy-boundaries-title">
          <div>
            <h3 id="routing-policy-boundaries-title" className="text-sm font-medium text-foreground">路由边界</h3>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
              <span>倍率上限</span>
              <input
                aria-label="倍率上限"
                className={inputClassName}
                type="number"
                min={0}
                step="any"
                value={config.maxRateMultiplier ?? ""}
                onChange={(event) => update("maxRateMultiplier", parseMaxRateMultiplier(event.target.value))}
              />
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
        <section className="grid gap-4 pb-5" aria-labelledby="routing-policy-weights-title">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 id="routing-policy-weights-title" className="text-sm font-medium text-foreground">评分权重</h3>
            </div>
            <div className="flex flex-wrap items-center justify-end gap-x-3 gap-y-1 text-xs">
              <StatusBadge tone={total === 10_000 ? "healthy" : "error"}>{`权重合计 ${total} / 10000`}</StatusBadge>
              {revision != null ? <span className="tabular-nums text-muted-foreground">revision {revision}</span> : null}
            </div>
          </div>
          <div className="grid grid-cols-2 gap-x-4 gap-y-3 lg:grid-cols-4">
            {WEIGHTS.map(({ key, label }) => (
              <label key={key} className="grid gap-1.5 text-xs font-medium text-muted-foreground">
                <span>{label}</span>
                <input className={weightInputClassName} type="number" min={0} max={10000} step={100} value={config[key]} onChange={(event) => update(key, Number(event.target.value))} />
              </label>
            ))}
          </div>
        </section>

        <section className="grid gap-4 py-5" aria-labelledby="routing-policy-runtime-title">
          <div>
            <h3 id="routing-policy-runtime-title" className="text-sm font-medium text-foreground">候选与探索</h3>
            <p className="mt-0.5 text-xs text-muted-foreground">控制每次请求的候选范围与探索额度。</p>
          </div>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
              <span>最大候选数</span>
              <input className={inputClassName} type="number" min={1} max={1024} value={config.maxCandidates} onChange={(event) => update("maxCandidates", Number(event.target.value))} />
            </label>
            <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
              <span>探索比例（基点）</span>
              <input className={inputClassName} type="number" min={0} max={2000} value={config.explorationShareBasisPoints} onChange={(event) => update("explorationShareBasisPoints", Number(event.target.value))} />
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
              <p className="mt-0.5 text-xs text-muted-foreground">在有效期内优先复用同一候选 密钥。</p>
            </div>
            <SwitchControl
              ariaLabel="启用会话亲和"
              checked={config.affinityEnabled}
              showLabel={false}
              onCheckedChange={() => update("affinityEnabled", !config.affinityEnabled)}
            />
          </div>
          <label className="grid max-w-xs gap-1.5 text-xs font-medium text-muted-foreground">
            <span>亲和 TTL（秒）</span>
            <input className={inputClassName} disabled={!config.affinityEnabled} type="number" min={1} max={86400} value={config.affinityTtlSeconds} onChange={(event) => update("affinityTtlSeconds", Number(event.target.value))} />
          </label>
        </section>

        <footer className="flex flex-wrap items-center justify-between gap-3 pt-4">
          <div className="min-h-5 text-xs" aria-live="polite">
            {error ? <p className="text-danger-foreground">{error}</p> : dirty ? <span className="text-muted-foreground">存在未保存的修改</span> : null}
          </div>
          <div className="flex gap-2">
            <Button type="button" variant="secondary" size="sm" disabled={!dirty || state === "saving"} onClick={() => { setConfig(saved); setState("idle"); }}><RefreshCw className="size-4" />撤销</Button>
            <Button type="button" size="sm" disabled={!dirty || total !== 10_000 || state === "saving"} onClick={() => void save()}><Save className="size-4" />保存策略</Button>
          </div>
        </footer>
      </div>
    </SectionCard>
  );
}
