import { useEffect, useMemo, useState } from "react";
import { RefreshCw, Save } from "lucide-react";
import { Button, SectionCard, StatusBadge, useToast } from "@/components/ui";
import { loadRoutingPolicy, updateRoutingPolicy } from "@/lib/api/routing";
import { readError } from "@/lib/errors";
import { refreshRoutingQueries } from "@/lib/query/routingQuerySynchronization";
import type { RoutingPolicyConfigV1 } from "@/lib/types/routing";
import { useQueryClient } from "@tanstack/react-query";

type SaveState = "idle" | "loading" | "dirty" | "saving" | "saved" | "error";

const WEIGHTS: Array<{ key: keyof Pick<RoutingPolicyConfigV1, "reliabilityWeight" | "responsivenessWeight" | "costWeight" | "preferenceWeight">; label: string }> = [
  { key: "reliabilityWeight", label: "可靠性" },
  { key: "responsivenessWeight", label: "响应速度" },
  { key: "costWeight", label: "成本" },
  { key: "preferenceWeight", label: "偏好" },
];

const inputClassName = "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface px-2.5 text-sm text-foreground outline-none focus:border-ring focus:ring-2 focus:ring-ring/30 disabled:bg-surface-subtle";

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

  const dirty = useMemo(() => JSON.stringify(config) !== JSON.stringify(saved), [config, saved]);
  const total = config
    ? config.reliabilityWeight + config.responsivenessWeight + config.costWeight + config.preferenceWeight
    : 0;

  function update<K extends keyof RoutingPolicyConfigV1>(key: K, value: RoutingPolicyConfigV1[K]) {
    setConfig((current) => current ? { ...current, [key]: value } : current);
    setState("dirty");
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
    <SectionCard title="智能路由策略" description="权重直接参与后端 Planner 的 utility 计算。权重总和必须为 10000。">
      <div className="grid gap-3">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          {WEIGHTS.map(({ key, label }) => (
            <label key={key} className="grid gap-1 text-xs text-muted-foreground">
              <span>{label}</span>
              <input className={inputClassName} type="number" min={0} max={10000} step={100} value={config[key]} onChange={(event) => update(key, Number(event.target.value))} />
            </label>
          ))}
        </div>
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <StatusBadge tone={total === 10_000 ? "healthy" : "error"}>{`权重合计 ${total} / 10000`}</StatusBadge>
          {revision != null ? <span className="text-muted-foreground">策略 revision {revision}</span> : null}
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <label className="grid gap-1 text-xs text-muted-foreground"><span>最大候选数</span><input className={inputClassName} type="number" min={1} max={1024} value={config.maxCandidates} onChange={(event) => update("maxCandidates", Number(event.target.value))} /></label>
          <label className="grid gap-1 text-xs text-muted-foreground"><span>探索比例（基点）</span><input className={inputClassName} type="number" min={0} max={2000} value={config.explorationShareBasisPoints} onChange={(event) => update("explorationShareBasisPoints", Number(event.target.value))} /></label>
          <label className="flex items-center gap-2 self-end pb-2 text-sm text-foreground"><input type="checkbox" checked={config.allowDepletedFallback} onChange={(event) => update("allowDepletedFallback", event.target.checked)} />允许耗尽余额作为应急回退</label>
        </div>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <label className="flex items-center gap-2 text-sm text-foreground"><input type="checkbox" checked={config.affinityEnabled} onChange={(event) => update("affinityEnabled", event.target.checked)} />启用会话亲和</label>
          <label className="grid gap-1 text-xs text-muted-foreground"><span>亲和 TTL（秒）</span><input className={inputClassName} disabled={!config.affinityEnabled} type="number" min={1} max={86400} value={config.affinityTtlSeconds} onChange={(event) => update("affinityTtlSeconds", Number(event.target.value))} /></label>
        </div>
        {error ? <p className="text-xs text-danger-foreground">{error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="secondary" size="sm" disabled={!dirty || state === "saving"} onClick={() => { setConfig(saved); setState("idle"); }}><RefreshCw className="size-4" />撤销</Button>
          <Button type="button" size="sm" disabled={!dirty || total !== 10_000 || state === "saving"} onClick={() => void save()}><Save className="size-4" />保存策略</Button>
        </div>
      </div>
    </SectionCard>
  );
}
