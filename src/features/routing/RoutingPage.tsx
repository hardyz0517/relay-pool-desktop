import { useEffect, useMemo, useState } from "react";
import { GitBranch, Play, Plus, RefreshCcw, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  EmptyState,
  InspectorPanel,
  IconButton,
  ObjectRow,
  SectionCard,
  SegmentedControl,
  StatusBadge,
} from "@/components/ui";
import {
  deleteModelAlias,
  listModelAliases,
  simulateRoute,
  upsertModelAlias,
} from "@/lib/api/routing";
import { getSettings, updateSettings } from "@/lib/api/settings";
import type {
  ModelAlias,
  RouteEndpointKind,
  RouteSimulationInput,
  RouteSimulationResult,
  RoutingPolicy,
  UpsertModelAliasInput,
} from "@/lib/types/routing";
import { routingStrategyLabels, type AppSettings } from "@/lib/types/settings";
import { cn } from "@/lib/utils";

const policyOptions: RoutingPolicy[] = ["priority_fallback", "stable_first", "backup_only"];

const endpointLabels: Record<RouteEndpointKind, string> = {
  models: "Models",
  chat_completions: "Chat Completions",
  responses: "Responses",
  embeddings: "Embeddings",
};

const fallbackSettings: AppSettings = {
  localProxyPort: 8787,
  localKeyMasked: "未读取",
  defaultRoutingStrategy: "priority_fallback",
  lowBalanceThresholdCny: 15,
  collectorIntervalMinutes: 30,
  trayBehavior: "minimize-to-tray",
  dataDir: "等待 Tauri 数据目录",
};

const emptyAliasForm: UpsertModelAliasInput = {
  id: null,
  clientModel: "",
  upstreamModel: "",
  enabled: true,
  note: null,
};

const defaultSimulation: RouteSimulationInput = {
  endpoint: "responses",
  model: "gpt-5.4",
  stream: true,
  usesTools: false,
  usesVision: false,
  usesReasoning: false,
  policy: null,
};

export function RoutingPage() {
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [aliases, setAliases] = useState<ModelAlias[]>([]);
  const [aliasForm, setAliasForm] = useState<UpsertModelAliasInput>(emptyAliasForm);
  const [simulation, setSimulation] = useState<RouteSimulationInput>(defaultSimulation);
  const [result, setResult] = useState<RouteSimulationResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  const acceptedCandidates = useMemo(
    () => result?.candidates.filter((candidate) => candidate.accepted) ?? [],
    [result],
  );
  const rejectedCandidates = useMemo(
    () => result?.candidates.filter((candidate) => !candidate.accepted) ?? [],
    [result],
  );

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      const [nextSettings, nextAliases] = await Promise.all([getSettings(), listModelAliases()]);
      setSettings(nextSettings);
      setAliases(nextAliases);
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setLoading(false);
    }
  }

  async function handlePolicyChange(policy: RoutingPolicy) {
    setSaving(true);
    setError(null);
    try {
      const nextSettings = await updateSettings({
        localProxyPort: settings.localProxyPort,
        defaultRoutingStrategy: policy,
        lowBalanceThresholdCny: settings.lowBalanceThresholdCny,
        collectorIntervalMinutes: settings.collectorIntervalMinutes,
        trayBehavior: settings.trayBehavior,
      });
      setSettings(nextSettings);
      setMessage("默认路由策略已保存。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleSaveAlias() {
    if (!aliasForm.clientModel.trim() || !aliasForm.upstreamModel.trim()) {
      setError("请填写客户端模型和上游模型。");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await upsertModelAlias({
        ...aliasForm,
        clientModel: aliasForm.clientModel.trim(),
        upstreamModel: aliasForm.upstreamModel.trim(),
        note: aliasForm.note?.trim() ? aliasForm.note.trim() : null,
      });
      setAliasForm(emptyAliasForm);
      setAliases(await listModelAliases());
      setMessage("模型映射已保存。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleToggleAlias(alias: ModelAlias) {
    setSaving(true);
    setError(null);
    try {
      await upsertModelAlias({
        id: alias.id,
        clientModel: alias.clientModel,
        upstreamModel: alias.upstreamModel,
        enabled: !alias.enabled,
        note: alias.note,
      });
      setAliases(await listModelAliases());
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleDeleteAlias(alias: ModelAlias) {
    if (!window.confirm(`确认删除模型映射「${alias.clientModel}」？`)) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await deleteModelAlias(alias.id);
      setAliases(await listModelAliases());
      setMessage("模型映射已删除。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleSimulate() {
    setSaving(true);
    setError(null);
    try {
      const nextResult = await simulateRoute({
        ...simulation,
        model: simulation.model?.trim() ? simulation.model.trim() : null,
      });
      setResult(nextResult);
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <PageScaffold
      title="路由规则"
      description="路由规则最终选择的是 Key 池中的 Station Key；P6 先按模型、协议、健康状态和简单策略解释选择原因。"
      actions={
        <Button disabled={loading || saving} variant="secondary" onClick={() => void refresh()}>
          <RefreshCcw className="h-4 w-4" />
          刷新
        </Button>
      }
    >
      <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_420px]">
        <div className="grid gap-3">
          <SectionCard title="默认策略" description="价格最优和余额避让不在 P6；当前策略只影响 Key 池候选排序。">
            <div className="flex flex-wrap items-center gap-3">
              <SegmentedControl
                value={settings.defaultRoutingStrategy}
                options={policyOptions.map((value) => ({
                  value,
                  label: routingStrategyLabels[value],
                }))}
                onChange={(value) => void handlePolicyChange(value)}
              />
              <StatusBadge tone={saving ? "warning" : "healthy"}>{saving ? "保存中" : "已同步"}</StatusBadge>
            </div>
            <div className="mt-3 text-xs leading-5 text-muted-foreground">
              当前默认策略：{routingStrategyLabels[settings.defaultRoutingStrategy]}。真实请求和模拟器都会从 enabled Key 中过滤协议、模型、冷却和能力冲突。
            </div>
          </SectionCard>

          <SectionCard
            title="模型映射"
            description="客户端模型名会在转发前映射为上游模型名；未匹配时保持原模型名。"
          >
            <div className="grid gap-3">
              <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_120px_auto]">
                <input
                  className={inputClassName}
                  value={aliasForm.clientModel}
                  placeholder="客户端模型，如 gpt-5.4"
                  onChange={(event) => setAliasForm({ ...aliasForm, clientModel: event.target.value })}
                />
                <input
                  className={inputClassName}
                  value={aliasForm.upstreamModel}
                  placeholder="上游模型，如 openai/gpt-5.4"
                  onChange={(event) => setAliasForm({ ...aliasForm, upstreamModel: event.target.value })}
                />
                <select
                  className={inputClassName}
                  value={aliasForm.enabled ? "enabled" : "disabled"}
                  onChange={(event) => setAliasForm({ ...aliasForm, enabled: event.target.value === "enabled" })}
                >
                  <option value="enabled">启用</option>
                  <option value="disabled">禁用</option>
                </select>
                <Button disabled={saving} type="button" onClick={() => void handleSaveAlias()}>
                  <Plus className="h-4 w-4" />
                  添加
                </Button>
              </div>
              <input
                className={inputClassName}
                value={aliasForm.note ?? ""}
                placeholder="备注，可选"
                onChange={(event) => setAliasForm({ ...aliasForm, note: event.target.value })}
              />
              {aliases.length === 0 ? (
                <EmptyState title="还没有模型映射" description="没有映射时，客户端模型名会原样传给上游。" />
              ) : (
                <div className="grid gap-2">
                  {aliases.map((alias) => (
                    <ObjectRow
                      key={alias.id}
                      icon={<GitBranch className="h-4 w-4" />}
                      title={alias.clientModel}
                      subtitle={`${alias.upstreamModel}${alias.note ? ` · ${alias.note}` : ""}`}
                      badges={<StatusBadge tone={alias.enabled ? "healthy" : "disabled"}>{alias.enabled ? "启用" : "禁用"}</StatusBadge>}
                      actions={
                        <>
                          <Button className="h-8" disabled={saving} variant="outline" onClick={() => void handleToggleAlias(alias)}>
                            {alias.enabled ? "停用" : "启用"}
                          </Button>
                          <IconButton label={`删除 ${alias.clientModel}`} disabled={saving} variant="danger" onClick={() => void handleDeleteAlias(alias)}>
                          <Trash2 className="h-4 w-4" />
                          </IconButton>
                        </>
                      }
                    />
                  ))}
                </div>
              )}
            </div>
          </SectionCard>
        </div>

        <InspectorPanel title="路由模拟器" description="不访问上游，只复用真实 selector 解释候选 Key。">
          <div className="grid gap-3 p-4">
            <Field label="Endpoint">
              <select className={inputClassName} value={simulation.endpoint} onChange={(event) => setSimulation({ ...simulation, endpoint: event.target.value as RouteEndpointKind })}>
                {(["responses", "chat_completions", "models", "embeddings"] as RouteEndpointKind[]).map((endpoint) => (
                  <option key={endpoint} value={endpoint}>{endpointLabels[endpoint]}</option>
                ))}
              </select>
            </Field>
            <Field label="模型">
              <input className={inputClassName} value={simulation.model ?? ""} onChange={(event) => setSimulation({ ...simulation, model: event.target.value })} placeholder="gpt-5.4" />
            </Field>
            <Field label="策略">
              <select className={inputClassName} value={simulation.policy ?? "default"} onChange={(event) => setSimulation({ ...simulation, policy: event.target.value === "default" ? null : event.target.value as RoutingPolicy })}>
                <option value="default">使用默认策略</option>
                {policyOptions.map((policy) => (
                  <option key={policy} value={policy}>{routingStrategyLabels[policy]}</option>
                ))}
              </select>
            </Field>
            <div className="grid gap-2 sm:grid-cols-2">
              <CheckField label="Stream" checked={simulation.stream} onChange={(checked) => setSimulation({ ...simulation, stream: checked })} />
              <CheckField label="Tools" checked={simulation.usesTools} onChange={(checked) => setSimulation({ ...simulation, usesTools: checked })} />
              <CheckField label="Vision" checked={simulation.usesVision} onChange={(checked) => setSimulation({ ...simulation, usesVision: checked })} />
              <CheckField label="Reasoning" checked={simulation.usesReasoning} onChange={(checked) => setSimulation({ ...simulation, usesReasoning: checked })} />
            </div>
            <Button disabled={saving} type="button" onClick={() => void handleSimulate()}>
              <Play className="h-4 w-4" />
              模拟路由
            </Button>
          </div>

          {result && (
            <div className="border-t border-cyan-100 p-4">
              <div className="text-sm font-semibold text-slate-800">{result.message}</div>
              <div className="mt-1 text-xs text-muted-foreground">
                策略：{routingStrategyLabels[result.policy]} · 映射模型：{result.mappedModel ?? "无"}
              </div>
              <CandidateList title="可用候选" candidates={acceptedCandidates} />
              <CandidateList title="拒绝候选" candidates={rejectedCandidates} />
            </div>
          )}
        </InspectorPanel>
      </div>

      {(message || error) && (
        <div
          className={cn(
            "fixed bottom-4 right-4 z-40 rounded-2xl border px-4 py-3 text-sm shadow-[0_12px_30px_rgba(33,79,88,0.12)]",
            error ? "border-rose-200 bg-rose-50 text-rose-700" : "border-emerald-200 bg-emerald-50 text-emerald-700",
          )}
        >
          {error ?? message}
        </div>
      )}
    </PageScaffold>
  );
}

function CandidateList({
  title,
  candidates,
}: {
  title: string;
  candidates: RouteSimulationResult["candidates"];
}) {
  if (candidates.length === 0) {
    return <div className="mt-3 text-xs text-muted-foreground">{title}：暂无</div>;
  }
  return (
    <div className="mt-4">
      <div className="mb-2 text-xs font-semibold text-slate-700">{title}</div>
      <div className="grid gap-2">
        {candidates.map((candidate, index) => (
          <ObjectRow
            key={candidate.stationKeyId}
            icon={<GitBranch className="h-4 w-4" />}
            title={candidate.stationName}
            subtitle={`${index + 1}. ${candidate.keyName} · ${candidate.mappedModel ?? "未映射模型"} · ${(candidate.accepted ? candidate.reasons : candidate.rejectionReasons).join("；") || "暂无原因"}`}
            badges={<StatusBadge tone={candidate.accepted ? "healthy" : "disabled"}>{candidate.accepted ? "可用" : "已过滤"}</StatusBadge>}
            metrics={[
              { label: "分数", value: candidate.score.toFixed(1), tone: candidate.accepted ? "good" : "neutral" },
              { label: "原因", value: `${candidate.reasons.length}` },
              { label: "过滤", value: `${candidate.rejectionReasons.length}`, tone: candidate.rejectionReasons.length > 0 ? "warning" : "neutral" },
            ]}
          />
        ))}
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}

function CheckField({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center gap-2 text-sm text-slate-700">
      <input
        checked={checked}
        className="h-4 w-4 accent-teal-600"
        type="checkbox"
        onChange={(event) => onChange(event.target.checked)}
      />
      {label}
    </label>
  );
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const inputClassName =
  "h-8 w-full rounded-xl border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
