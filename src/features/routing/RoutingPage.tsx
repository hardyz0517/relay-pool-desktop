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
  SelectControl,
  StatusBadge,
  useToast,
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

const policyOptions: RoutingPolicy[] = ["priority_fallback", "stable_first", "backup_only", "cheap_first"];

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
  const toast = useToast();
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [aliases, setAliases] = useState<ModelAlias[]>([]);
  const [aliasForm, setAliasForm] = useState<UpsertModelAliasInput>(emptyAliasForm);
  const [simulation, setSimulation] = useState<RouteSimulationInput>(defaultSimulation);
  const [result, setResult] = useState<RouteSimulationResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
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
      const message = readError(requestError);
      setError(message);
      toast.error("刷新路由规则失败", message);
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
      toast.success("默认策略已保存");
    } catch (requestError) {
      toast.error("保存默认策略失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleSaveAlias() {
    if (!aliasForm.clientModel.trim() || !aliasForm.upstreamModel.trim()) {
      toast.info("请填写客户端模型和上游模型");
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
      toast.success("模型映射已保存");
    } catch (requestError) {
      toast.error("保存模型映射失败", readError(requestError));
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
      toast.success(alias.enabled ? "模型映射已停用" : "模型映射已启用");
    } catch (requestError) {
      toast.error("更新模型映射失败", readError(requestError));
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
      toast.success("模型映射已删除");
    } catch (requestError) {
      toast.error("删除模型映射失败", readError(requestError));
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
      toast.success("路由模拟完成");
    } catch (requestError) {
      toast.error("路由模拟失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <PageScaffold
      title="路由规则"
      description="路由最终选择的是 Key 池中的 Station Key；P7 只负责默认策略和解释，不引入复杂策略编辑。"
      actions={
        <Button disabled={loading || saving} variant="secondary" onClick={() => void refresh()}>
          <RefreshCcw className="h-4 w-4" />
          刷新
        </Button>
      }
    >
      <div className="grid gap-3">
        <div className="grid gap-3">
          <SectionCard title="默认策略" description="当前策略只影响 Key 池候选排序。">
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
            description="客户端模型名会在转发前映射为上游模型名；未命中时保持原模型名。"
          >
            <div className="grid gap-3">
              <div className="grid gap-2">
                <input
                  className={inputClassName}
                  value={aliasForm.clientModel}
                  placeholder="客户端模型，例如 gpt-5.4"
                  onChange={(event) => setAliasForm({ ...aliasForm, clientModel: event.target.value })}
                />
                <input
                  className={inputClassName}
                  value={aliasForm.upstreamModel}
                  placeholder="上游模型，例如 openai/gpt-5.4"
                  onChange={(event) => setAliasForm({ ...aliasForm, upstreamModel: event.target.value })}
                />
                <SelectControl
                  ariaLabel="模型映射状态"
                  className={inputClassName}
                  value={aliasForm.enabled ? "enabled" : "disabled"}
                  options={[
                    { value: "enabled", label: "启用" },
                    { value: "disabled", label: "禁用" },
                  ]}
                  onChange={(enabledState) => setAliasForm({ ...aliasForm, enabled: enabledState === "enabled" })}
                />
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
                <EmptyState title="还没有模型映射" description="没有映射时，客户端模型会原样传给上游。" />
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

        <InspectorPanel title="路由模拟器" description="用真实 selector 解释候选排序。">
          <div className="grid gap-3 p-4">
            <Field label="Endpoint">
              <SelectControl
                ariaLabel="模拟 endpoint"
                className={inputClassName}
                value={simulation.endpoint}
                options={(["responses", "chat_completions", "models", "embeddings"] as RouteEndpointKind[]).map((endpoint) => ({
                  value: endpoint,
                  label: endpointLabels[endpoint],
                }))}
                onChange={(endpoint) => setSimulation({ ...simulation, endpoint })}
              />
            </Field>
            <Field label="模型">
              <input className={inputClassName} value={simulation.model ?? ""} onChange={(event) => setSimulation({ ...simulation, model: event.target.value })} placeholder="gpt-5.4" />
            </Field>
            <Field label="策略">
              <SelectControl
                ariaLabel="模拟路由策略"
                className={inputClassName}
                value={simulation.policy ?? "default"}
                options={[
                  { value: "default", label: "使用默认策略" },
                  ...policyOptions.map((policy) => ({
                    value: policy,
                    label: routingStrategyLabels[policy],
                  })),
                ]}
                onChange={(policy) => setSimulation({ ...simulation, policy: policy === "default" ? null : policy })}
              />
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
            subtitle={`${index + 1}. ${candidate.keyName} · ${candidate.mappedModel ?? "未映射模型"} · ${candidateSummary(candidate)}`}
            badges={<StatusBadge tone={candidate.accepted ? "healthy" : "disabled"}>{candidate.accepted ? "可用" : "已过滤"}</StatusBadge>}
            metrics={[
              { label: "分数", value: candidate.score.toFixed(1), tone: candidate.accepted ? "good" : "neutral" },
              { label: "成本", value: formatCandidateCost(candidate), tone: candidate.estimatedOutputPrice == null ? "neutral" : "good" },
              { label: "余额", value: formatCandidateBalance(candidate), tone: candidate.balanceStatus === "low" || candidate.balanceStatus === "depleted" ? "warning" : "neutral" },
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

function candidateSummary(candidate: RouteSimulationResult["candidates"][number]) {
  const routeReasons = candidate.accepted ? candidate.reasons : candidate.rejectionReasons;
  return [...routeReasons, ...candidate.economicReasons].join("；") || "暂无原因";
}

function formatCandidateCost(candidate: RouteSimulationResult["candidates"][number]) {
  if (candidate.estimatedOutputPrice == null) {
    return "-";
  }
  return `${candidate.priceCurrency ?? "USD"} ${candidate.estimatedOutputPrice.toFixed(4)}`;
}

function formatCandidateBalance(candidate: RouteSimulationResult["candidates"][number]) {
  if (candidate.balanceValue == null) {
    return candidate.balanceStatus ?? "-";
  }
  return `${candidate.balanceValue.toFixed(2)}${candidate.balanceStatus ? ` / ${candidate.balanceStatus}` : ""}`;
}
