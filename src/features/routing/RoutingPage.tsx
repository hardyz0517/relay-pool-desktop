import { useEffect, useMemo, useState } from "react";
import { GitBranch, Play, Plus, RefreshCcw, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  ConfirmDialog,
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
import { readError } from "@/lib/errors";
import { formatRate } from "@/lib/formatters";
import { loadLocalRoutingWorkspace } from "@/lib/queries/localRoutingQueries";
import { loadRoutingWorkspace } from "@/lib/queries/routingQueries";
import {
  deleteModelAlias,
  listModelAliases,
  simulateRoute,
  upsertModelAlias,
} from "@/lib/api/routing";
import { updateSettings } from "@/lib/api/settings";
import type {
  ModelAlias,
  RouteEndpointKind,
  RouteSimulationInput,
  RouteSimulationResult,
  RoutingPolicy,
  UpsertModelAliasInput,
} from "@/lib/types/routing";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
import type { AppRouteId } from "@/lib/types/navigation";
import { routingStrategyLabels, type AppSettings } from "@/lib/types/settings";
import { cn } from "@/lib/utils";
import { LocalRoutingEditTab } from "./LocalRoutingEditTab";
import { LocalRoutingStatusTab } from "./LocalRoutingStatusTab";

const policyOptions: RoutingPolicy[] = ["priority_fallback", "stable_first", "backup_only", "cheap_first"];

const endpointLabels: Record<RouteEndpointKind, string> = {
  models: "模型列表",
  chat_completions: "聊天补全",
  responses: "响应接口",
  embeddings: "向量接口",
};

const fallbackSettings: AppSettings = {
  localProxyPort: 8787,
  localKeyMasked: "未读取",
  defaultRoutingStrategy: "priority_fallback",
  lowBalanceThresholdCny: 15,
  collectorIntervalMinutes: 30,
  balanceIntervalMinutes: 5,
  groupRateIntervalMinutes: 20,
  modelListIntervalMinutes: 60,
  pricingRefreshIntervalMinutes: 60,
  collectorTimeoutSeconds: 15,
  collectorMaxConcurrency: 3,
  allowDepletedFallback: false,
  trayBehavior: "minimize-to-tray",
  developerModeEnabled: false,
  dataDir: "仅桌面端可读取",
  pendingDataDir: null,
  dataDirChangeRequiresRestart: false,
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

type LocalRoutingTab = "status" | "edit";
type LocalRoutingLinkedPage = Extract<AppRouteId, "channels" | "logs">;

type RoutingPageProps = {
  onOpenPage?: (pageId: LocalRoutingLinkedPage) => void;
};

export function RoutingPage({ onOpenPage }: RoutingPageProps) {
  const toast = useToast();
  const [activeTab, setActiveTab] = useState<LocalRoutingTab>("status");
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [localWorkspace, setLocalWorkspace] = useState<LocalRoutingWorkspace | null>(null);
  const [aliases, setAliases] = useState<ModelAlias[]>([]);
  const [aliasForm, setAliasForm] = useState<UpsertModelAliasInput>(emptyAliasForm);
  const [simulation, setSimulation] = useState<RouteSimulationInput>(defaultSimulation);
  const [result, setResult] = useState<RouteSimulationResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [pendingDeleteAlias, setPendingDeleteAlias] = useState<ModelAlias | null>(null);
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
      const routingWorkspace = await loadRoutingWorkspace();
      setSettings(routingWorkspace.settings);
      setAliases(routingWorkspace.modelAliases);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新路由规则失败", message);
      setLoading(false);
      return;
    }

    try {
      setLocalWorkspace(await loadLocalRoutingWorkspace());
    } catch (requestError) {
      const message = readError(requestError);
      setLocalWorkspace(null);
      toast.error("刷新本地路由状态失败", message);
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
        balanceIntervalMinutes: settings.balanceIntervalMinutes,
        groupRateIntervalMinutes: settings.groupRateIntervalMinutes,
        modelListIntervalMinutes: settings.modelListIntervalMinutes,
        pricingRefreshIntervalMinutes: settings.pricingRefreshIntervalMinutes,
        collectorTimeoutSeconds: settings.collectorTimeoutSeconds,
        collectorMaxConcurrency: settings.collectorMaxConcurrency,
        allowDepletedFallback: settings.allowDepletedFallback,
        trayBehavior: settings.trayBehavior,
        developerModeEnabled: settings.developerModeEnabled,
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

  function handleDeleteAlias(alias: ModelAlias) {
    setPendingDeleteAlias(alias);
  }

  async function handleConfirmDeleteAlias() {
    if (!pendingDeleteAlias) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await deleteModelAlias(pendingDeleteAlias.id);
      setAliases(await listModelAliases());
      setPendingDeleteAlias(null);
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
      actions={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <SegmentedControl
            ariaLabel="本地路由页面"
            value={activeTab}
            options={[
              { value: "status", label: "状态" },
              { value: "edit", label: "编辑" },
            ]}
            onChange={setActiveTab}
          />
          <Button disabled={loading || saving} variant="secondary" onClick={() => void refresh()}>
            <RefreshCcw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      <div className="grid gap-3">
        {activeTab === "status" ? (
          <LocalRoutingStatusTab loading={loading} workspace={localWorkspace} onOpenPage={onOpenPage} />
        ) : (
          <LocalRoutingEditTab loading={loading} workspace={localWorkspace} />
        )}

        <div className="grid gap-3">
          <SectionCard title="默认策略">
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
              当前默认策略：{routingStrategyLabels[settings.defaultRoutingStrategy]}。真实请求和模拟器都会过滤协议、模型、冷却和能力冲突。
            </div>
          </SectionCard>

          <SectionCard
            title="模型映射"
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
                          <IconButton label={`删除 ${alias.clientModel}`} disabled={saving} variant="danger" onClick={() => handleDeleteAlias(alias)}>
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

        <InspectorPanel title="路由模拟器">
          <div className="grid gap-3 p-4">
            <Field label="接口">
              <SelectControl
                ariaLabel="模拟接口"
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
              <CheckField label="流式响应" checked={simulation.stream} onChange={(checked) => setSimulation({ ...simulation, stream: checked })} />
              <CheckField label="工具调用" checked={simulation.usesTools} onChange={(checked) => setSimulation({ ...simulation, usesTools: checked })} />
              <CheckField label="图片输入" checked={simulation.usesVision} onChange={(checked) => setSimulation({ ...simulation, usesVision: checked })} />
              <CheckField label="推理模型" checked={simulation.usesReasoning} onChange={(checked) => setSimulation({ ...simulation, usesReasoning: checked })} />
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
      <ConfirmDialog
        open={pendingDeleteAlias !== null}
        title="删除模型映射"
        description={`确定要删除模型映射 "${pendingDeleteAlias?.clientModel ?? ""}" 吗？此操作无法撤销。`}
        confirmLabel="删除"
        confirming={saving}
        onCancel={() => setPendingDeleteAlias(null)}
        onConfirm={() => void handleConfirmDeleteAlias()}
      />

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
          <div key={candidate.stationKeyId} className="rounded-[var(--surface-radius)] border border-border bg-white">
            <ObjectRow
              className="rounded-b-none border-0 border-b border-border"
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
            <CandidateEconomicsDetails candidate={candidate} />
          </div>
        ))}
      </div>
    </div>
  );
}

function CandidateEconomicsDetails({ candidate }: { candidate: RouteSimulationResult["candidates"][number] }) {
  return (
    <div className="grid gap-2 px-3 py-2 text-xs text-muted-foreground md:grid-cols-2">
      <div>分组：{candidate.groupBindingId ?? "未绑定"}</div>
      <div>倍率：{candidate.rateMultiplier == null ? "未知" : formatRate(candidate.rateMultiplier)}</div>
      <div>价格：{normalizationLabel(candidate.normalizationStatus)}</div>
      <div>余额：{candidate.balanceStatus ?? "未知"} · {candidate.balanceScope ?? "未知"}</div>
      <div>新鲜度：{candidate.economicFreshness ?? "未知"}</div>
      <div>规则：{candidate.pricingRuleId ?? "未命中"}</div>
      {candidate.rejectionReasons.length > 0 && (
        <div className="flex flex-wrap gap-1 md:col-span-2">
          {candidate.rejectionReasons.map((reason) => (
            <StatusBadge key={reason} className="h-5 px-1.5" tone="warning">{reason}</StatusBadge>
          ))}
        </div>
      )}
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

function normalizationLabel(value: string | null | undefined) {
  if (!value) return "未知";
  if (value === "complete") return "完整";
  if (value === "group_rate_only") return "仅倍率";
  if (value === "expired") return "已过期";
  return value;
}
