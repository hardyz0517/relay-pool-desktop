import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { AlertTriangle, ArrowLeft, Check, Plus, RefreshCw, X } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, IconButton, PageForm, SectionCard, SelectControl, SwitchControl } from "@/components/ui";
import { getStationKeyCapabilities } from "@/lib/api/routing";
import { readError } from "@/lib/errors";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type {
  ChannelMonitor,
  ChannelMonitorHealthWritebackMode,
  ChannelMonitorProtocolKind,
  ChannelMonitorProxyMode,
  ChannelMonitorRequestTemplate,
  CreateChannelMonitorInput,
  MonitoringCapabilityCatalog,
} from "@/lib/types/channelMonitors";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import {
  createEmptyMonitorDraft,
  draftToMonitorInput,
  monitorToDraft,
  targetTypeOptions,
  templateForMonitorProtocol,
  validateMonitorDraft,
  type ChannelMonitorDraft,
} from "@/lib/channelMonitorViewModel";
import { MonitorProfileSelector } from "./components/MonitorProfileSelector";
import { MonitorProtocolSelector } from "./components/MonitorProtocolSelector";

type ChannelMonitorFormProps = {
  monitor: ChannelMonitor | null;
  stations: Station[];
  keys: KeyPoolItem[];
  templates: ChannelMonitorRequestTemplate[];
  capabilities: MonitoringCapabilityCatalog | undefined;
  capabilitiesError: string | null;
  saving: boolean;
  onClose: () => void;
  onRetryCapabilities: () => void;
  onSubmit: (input: CreateChannelMonitorInput) => Promise<void> | void;
};

const inputClassName =
  "h-8 rounded-[8px] border border-border bg-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/20";

const healthWritebackOptions: Array<{
  value: ChannelMonitorHealthWritebackMode;
  label: string;
  description: string;
}> = [
  { value: "disabled", label: "不写回", description: "只保留监控结果，不影响密钥健康状态" },
  { value: "observe_only", label: "仅观察", description: "记录健康观察，但不改变路由资格" },
  { value: "authoritative", label: "权威写回", description: "达到阈值后更新密钥健康状态，仅限标准 API Profile" },
];

const monitorProxyOptions: Array<{ value: ChannelMonitorProxyMode; label: string }> = [
  { value: "inherit", label: "跟随总设置" },
  { value: "direct", label: "直连" },
  { value: "system", label: "使用系统代理" },
  { value: "manual", label: "手动代理地址" },
];

export function ChannelMonitorForm({
  monitor,
  stations,
  keys,
  templates,
  capabilities,
  capabilitiesError,
  saving,
  onClose,
  onRetryCapabilities,
  onSubmit,
}: ChannelMonitorFormProps) {
  const [draft, setDraft] = useState<ChannelMonitorDraft>(() =>
    monitor
      ? monitorToDraft(monitor)
      : createEmptyMonitorDraft(stations, templates, capabilities),
  );
  const [riskAcknowledged, setRiskAcknowledged] = useState(false);

  useEffect(() => {
    if (!capabilities || monitor) return;
    setDraft((current) => {
      const selectedProtocol = capabilities.protocols.find((protocol) =>
        protocol.id === current.protocolKind && protocol.enabled);
      const protocolKind = (selectedProtocol?.id
        ?? capabilities.protocols.find((protocol) => protocol.enabled)?.id
        ?? current.protocolKind) as ChannelMonitorProtocolKind;
      const selectedProfile = capabilities.profiles.find((profile) =>
        profile.id === current.clientProfileId
        && profile.enabled
        && profile.supportedProtocols.includes(protocolKind));
      const profile = selectedProfile ?? capabilities.profiles.find((item) =>
        item.id === "standard_api"
        && item.enabled
        && item.supportedProtocols.includes(protocolKind));
      return {
        ...current,
        protocolKind,
        templateId: templateForMonitorProtocol(templates, protocolKind)?.id ?? current.templateId,
        ...(profile
          ? { clientProfileId: profile.id as ChannelMonitorDraft["clientProfileId"], clientProfileVersion: String(profile.version) }
          : {}),
      };
    });
  }, [capabilities, monitor, templates]);

  const stationOptions = useMemo(
    () => stations.map((station) => ({ value: station.id, label: station.name })),
    [stations],
  );
  const stationKeys = useMemo(
    () => keys.filter((key) => key.stationId === draft.stationId),
    [draft.stationId, keys],
  );
  const keyOptions = useMemo(
    () => stationKeys.map((key) => ({
      value: key.id,
      label: key.name,
      description: key.enabled ? key.modelScopeSummary || key.groupName || "全部模型" : "已停用",
      disabled: !key.enabled,
    })),
    [stationKeys],
  );
  const modelKeyIds = useMemo(
    () => draft.targetType === "station_key"
      ? (draft.stationKeyId ? [draft.stationKeyId] : [])
      : stationKeys.filter((key) => key.enabled).map((key) => key.id),
    [draft.stationKeyId, draft.targetType, stationKeys],
  );
  const modelCapabilitiesQuery = useActivityQuery({
    queryKey: ["channelMonitoring", "keyCapabilities", modelKeyIds],
    enabled: modelKeyIds.length > 0,
    queryFn: () => Promise.all(modelKeyIds.map((stationKeyId) => getStationKeyCapabilities(stationKeyId))),
    staleTime: 5_000,
  });
  const modelOptions = useMemo(
    () => buildMonitorModelOptions(modelCapabilitiesQuery.data ?? []),
    [modelCapabilitiesQuery.data],
  );
  const modelOptionsUnavailable = modelCapabilitiesQuery.isError || modelCapabilitiesQuery.isPending || modelKeyIds.length === 0;
  const selectedProfile = capabilities?.profiles.find((profile) => profile.id === draft.clientProfileId);
  const validationError = validateMonitorDraft(draft, { templates, keys, capabilities });
  const highRisk = draft.healthPolicyMode === "authoritative" || Number(draft.intervalSeconds) < 60;
  const canSubmit = !validationError && !saving && (!highRisk || riskAcknowledged);
  const isStationTarget = draft.targetType === "station";
  function updateDraft(patch: Partial<ChannelMonitorDraft>) {
    setDraft((current) => ({ ...current, ...patch }));
  }

  function handleStationChange(stationId: string) {
    const firstKey = keys.find((key) => key.stationId === stationId && key.enabled)?.id ?? "";
    updateDraft({
      stationId,
      stationKeyId: draft.targetType === "station_key" ? firstKey : "",
    });
  }

  function handleTargetTypeChange(targetType: ChannelMonitorDraft["targetType"]) {
    updateDraft({
      targetType,
      stationKeyId: targetType === "station_key" ? stationKeys.find((key) => key.enabled)?.id ?? "" : "",
    });
  }

  function handleProtocolChange(protocolKind: ChannelMonitorProtocolKind) {
    const template = templateForMonitorProtocol(templates, protocolKind);
    const currentProfileCompatible = selectedProfile?.enabled
      && selectedProfile.supportedProtocols.includes(protocolKind);
    const standardProfile = capabilities?.profiles.find((profile) =>
      profile.id === "standard_api" && profile.enabled && profile.supportedProtocols.includes(protocolKind));
    updateDraft({
      protocolKind,
      templateId: template?.id ?? draft.templateId,
      ...(currentProfileCompatible || !standardProfile
        ? {}
        : { clientProfileId: "standard_api", clientProfileVersion: String(standardProfile.version) }),
    });
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSubmit) return;
    await onSubmit(draftToMonitorInput(draft));
  }

  return (
    <PageScaffold
      title={monitor ? "编辑渠道监控" : "新增渠道监控"}
      fill
      stickyHeader
      backAction={
        <IconButton label="返回监控列表" onClick={onClose} disabled={saving}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
    >
      <PageForm
        id="channel-monitor-form"
        className="w-full"
        onSubmit={handleSubmit}
        footer={
          <>
            <Button variant="secondary" onClick={onClose} disabled={saving}>取消</Button>
            <Button type="submit" disabled={!canSubmit}>
              <Check className="h-4 w-4" />
              {saving ? "保存中" : "保存"}
            </Button>
          </>
        }
      >
        <section className="grid gap-[var(--shell-page-gap)]">
          <SectionCard title="监控目标">
            <div className="grid gap-3 md:grid-cols-[minmax(0,1.3fr)_12rem_12rem]">
              <Field label="监控名称">
                <input className={inputClassName} value={draft.name} onChange={(event) => updateDraft({ name: event.target.value })} />
              </Field>
              <Field label="目标类型">
                <SelectControl ariaLabel="目标类型" className={inputClassName} value={draft.targetType} options={targetTypeOptions} onChange={handleTargetTypeChange} />
              </Field>
              <Field label="启用状态">
                <SwitchControl checked={draft.enabled} ariaLabel="启用监控" onCheckedChange={() => updateDraft({ enabled: !draft.enabled })} onLabel="启用" offLabel="停用" className="h-8" />
              </Field>
            </div>
            <div className="mt-3 grid gap-3 md:grid-cols-2">
              <Field label="中转站">
                <SelectControl ariaLabel="中转站" className={inputClassName} value={draft.stationId} options={stationOptions} placeholder="请选择中转站" onChange={handleStationChange} />
              </Field>
              <Field label="站点密钥">
                <SelectControl ariaLabel="站点密钥" className={inputClassName} value={draft.stationKeyId} options={keyOptions} placeholder={isStationTarget ? "中转站目标不需要选择密钥" : "请选择密钥"} disabled={isStationTarget} onChange={(stationKeyId) => updateDraft({ stationKeyId })} />
              </Field>
            </div>
          </SectionCard>

          <SectionCard title="探测请求">
            {!capabilities && (
              <div className={`mb-3 flex min-h-10 flex-wrap items-center justify-between gap-2 rounded-[var(--surface-radius)] border px-3 py-2 text-sm ${
                capabilitiesError
                  ? "border-danger-border bg-danger-surface text-danger-foreground"
                  : "border-border bg-surface-subtle text-muted-foreground"
              }`}>
                <span>{capabilitiesError ? `监控能力加载失败：${capabilitiesError}` : "正在加载协议与 Profile 能力"}</span>
                {capabilitiesError && (
                  <Button size="sm" variant="secondary" onClick={onRetryCapabilities}>
                    <RefreshCw className="h-3.5 w-3.5" />
                    重试
                  </Button>
                )}
              </div>
            )}
            <Field label="请求协议">
              <MonitorProtocolSelector value={draft.protocolKind} capabilities={capabilities} onChange={handleProtocolChange} />
            </Field>
            <div className="mt-3 grid gap-3 md:grid-cols-2">
              <Field label="请求 Profile">
                <MonitorProfileSelector
                  value={draft.clientProfileId}
                  protocolKind={draft.protocolKind}
                  capabilities={capabilities}
                  onChange={(clientProfileId, version) => updateDraft({ clientProfileId, clientProfileVersion: String(version) })}
                />
              </Field>
              <Field label="主模型">
                <ModelInput
                  ariaLabel="主模型"
                  selectAriaLabel="从模型列表选择主模型"
                  value={draft.primaryModel}
                  options={modelOptions}
                  selectDisabled={modelOptionsUnavailable}
                  placeholder="例如 gpt-4.1-mini"
                  onChange={(primaryModel) => updateDraft({ primaryModel })}
                />
                {modelCapabilitiesQuery.isPending && modelKeyIds.length > 0 ? (
                  <span className="text-[11px] font-normal text-muted-foreground">正在读取密钥已获取的模型…</span>
                ) : null}
                {modelCapabilitiesQuery.isError ? (
                  <span className="text-[11px] font-normal text-danger-foreground" role="alert">
                    读取密钥模型列表失败：{readError(modelCapabilitiesQuery.error)}
                  </span>
                ) : null}
                {!modelCapabilitiesQuery.isPending && !modelCapabilitiesQuery.isError && modelKeyIds.length > 0 && modelOptions.length === 0 ? (
                  <span className="text-[11px] font-normal text-muted-foreground">该密钥尚未拉取模型列表，可手动输入模型名称。</span>
                ) : null}
              </Field>
            </div>
            <FallbackModelEditor
              models={draft.fallbackModels}
              options={modelOptions}
              selectDisabled={modelOptionsUnavailable}
              onChange={(fallbackModels) => updateDraft({ fallbackModels })}
            />
          </SectionCard>

          <SectionCard title="调度与预算">
            <div className="grid gap-3 md:grid-cols-3 xl:grid-cols-6">
              <Field label="间隔（秒）"><NumberInput value={draft.intervalSeconds} onChange={(intervalSeconds) => updateDraft({ intervalSeconds })} /></Field>
              <Field label="抖动（秒）"><NumberInput value={draft.jitterSeconds} onChange={(jitterSeconds) => updateDraft({ jitterSeconds })} /></Field>
              <Field label="单次超时（毫秒）"><NumberInput value={draft.attemptTimeoutMs} onChange={(attemptTimeoutMs) => updateDraft({ attemptTimeoutMs })} /></Field>
              <Field label="任务超时（毫秒）"><NumberInput value={draft.executionTimeoutMs} onChange={(executionTimeoutMs) => updateDraft({ executionTimeoutMs })} /></Field>
              <Field label="每日尝试次数上限"><NumberInput value={draft.riskDailyProbeBudget} onChange={(riskDailyProbeBudget) => updateDraft({ riskDailyProbeBudget })} /></Field>
              <Field label="零余额自动暂停">
                <SwitchControl
                  checked={draft.pauseOnZeroBalance}
                  ariaLabel="余额为零时自动暂停监控"
                  onCheckedChange={() => updateDraft({ pauseOnZeroBalance: !draft.pauseOnZeroBalance })}
                  showLabel={false}
                />
              </Field>
            </div>
          </SectionCard>

          <SectionCard title="重试与健康">
            <div className="grid gap-3 md:grid-cols-3">
              <Field label="每模型尝试次数"><NumberInput value={draft.retryMaxAttemptsPerModel} onChange={(retryMaxAttemptsPerModel) => updateDraft({ retryMaxAttemptsPerModel })} /></Field>
              <Field label="首次退避（毫秒）"><NumberInput value={draft.retryInitialBackoffMs} onChange={(retryInitialBackoffMs) => updateDraft({ retryInitialBackoffMs })} /></Field>
              <Field label="最大退避（毫秒）"><NumberInput value={draft.retryMaxBackoffMs} onChange={(retryMaxBackoffMs) => updateDraft({ retryMaxBackoffMs })} /></Field>
            </div>
            <div className="mt-3 grid gap-3 md:grid-cols-3">
              <Field label="健康写回">
                <SelectControl ariaLabel="健康写回" className={inputClassName} value={draft.healthPolicyMode} options={healthWritebackOptions} onChange={(healthPolicyMode) => updateDraft({ healthPolicyMode })} />
              </Field>
              <Field label="连续失败阈值"><NumberInput value={draft.healthFailureThreshold} onChange={(healthFailureThreshold) => updateDraft({ healthFailureThreshold })} /></Field>
              <Field label="连续恢复阈值"><NumberInput value={draft.healthRecoveryThreshold} onChange={(healthRecoveryThreshold) => updateDraft({ healthRecoveryThreshold })} /></Field>
            </div>
          </SectionCard>

          <SectionCard title="网络与代理">
            <div className="grid gap-3 md:grid-cols-2">
              <Field label="监控网络出口">
                <SelectControl
                  ariaLabel="监控网络出口"
                  className={inputClassName}
                  value={draft.proxyMode}
                  options={monitorProxyOptions}
                  onChange={(proxyMode) => updateDraft({ proxyMode })}
                />
              </Field>
              {draft.proxyMode === "manual" ? (
                <Field label="代理地址">
                  <input
                    className={inputClassName}
                    placeholder="例如 http://127.0.0.1:7890"
                    value={draft.proxyUrl}
                    onChange={(event) => updateDraft({ proxyUrl: event.target.value })}
                  />
                </Field>
              ) : (
                <div className="flex items-end pb-1 text-xs text-muted-foreground">
                  {draft.proxyMode === "inherit" ? "当前监控请求会使用总设置中的网络出口。" : "当前监控请求会覆盖总设置。"}
                </div>
              )}
            </div>
          </SectionCard>

          <SectionCard title="备注与确认">
            <Field label="备注">
              <textarea className={`${inputClassName} min-h-20 resize-none py-2`} value={draft.note} onChange={(event) => updateDraft({ note: event.target.value })} />
            </Field>
            {highRisk && (
              <label className="mt-3 flex items-start gap-2 rounded-[var(--surface-radius)] border border-warning-border bg-warning-surface px-3 py-2 text-sm text-warning-foreground">
                <input className="mt-0.5 h-4 w-4 accent-primary" type="checkbox" checked={riskAcknowledged} onChange={(event) => setRiskAcknowledged(event.target.checked)} />
                <span className="flex gap-2">
                  <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                  此配置使用高频探测或权威健康写回。我已确认渠道许可、请求预算和健康影响。
                </span>
              </label>
            )}
            {validationError && (
              <div className="mt-3 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">{validationError}</div>
            )}
          </SectionCard>
        </section>
      </PageForm>
    </PageScaffold>
  );
}

function FallbackModelEditor({
  models,
  options,
  selectDisabled,
  onChange,
}: {
  models: string[];
  options: string[];
  selectDisabled: boolean;
  onChange: (models: string[]) => void;
}) {
  return (
    <div className="mt-3">
      <div className="mb-1.5 flex items-center justify-between gap-2 text-xs font-medium text-muted-foreground">
        <span>回退模型（最多 3 个，按顺序尝试）</span>
        {models.length < 3 && (
          <Button size="sm" variant="ghost" onClick={() => onChange([...models, ""])}>
            <Plus className="h-3.5 w-3.5" />添加模型
          </Button>
        )}
      </div>
      {models.length === 0 ? (
        <div className="rounded-[8px] border border-dashed border-border px-3 py-2 text-xs text-muted-foreground">未配置回退模型</div>
      ) : (
        <div className="grid gap-2 md:grid-cols-3">
          {models.map((model, index) => (
            <div key={index} className="flex min-w-0 items-center gap-1.5">
              <ModelInput
                ariaLabel={`回退模型 ${index + 1}`}
                selectAriaLabel={`从模型列表选择回退模型 ${index + 1}`}
                value={model}
                options={options}
                selectDisabled={selectDisabled}
                placeholder={`回退模型 ${index + 1}`}
                onChange={(nextModel) => onChange(models.map((item, itemIndex) => itemIndex === index ? nextModel : item))}
              />
              <IconButton label={`移除回退模型 ${index + 1}`} onClick={() => onChange(models.filter((_, itemIndex) => itemIndex !== index))}>
                <X className="h-4 w-4" />
              </IconButton>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function ModelInput({
  ariaLabel,
  selectAriaLabel,
  value,
  options,
  selectDisabled = false,
  placeholder,
  onChange,
}: {
  ariaLabel: string;
  selectAriaLabel: string;
  value: string;
  options: string[];
  selectDisabled?: boolean;
  placeholder: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="grid min-w-0 flex-1 grid-cols-[minmax(0,1fr)_auto] gap-2">
      <input
        aria-label={ariaLabel}
        className={`${inputClassName} min-w-0 w-full`}
        value={value}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
      />
      <SelectControl
        ariaLabel={selectAriaLabel}
        title="从模型列表选择"
        className="h-8 w-8 min-w-[2rem] justify-center gap-0 px-0 shadow-none"
        disabled={selectDisabled || options.length === 0}
        menuAlign="end"
        menuMinWidth={220}
        options={options.map((model) => ({ value: model, label: model }))}
        placeholder={null}
        searchable
        searchPlaceholder="搜索模型..."
        emptyLabel="没有匹配的模型"
        value=""
        onChange={onChange}
      />
    </div>
  );
}

function buildMonitorModelOptions(capabilities: Array<{ modelAllowlist: string[]; modelBlocklist: string[] }>) {
  const firstAllowlist = capabilities[0]?.modelAllowlist ?? [];
  const availableInEveryKey = new Set(firstAllowlist.map((model) => model.trim().toLocaleLowerCase()).filter(Boolean));
  for (const capability of capabilities) {
    const allowlist = new Set(capability.modelAllowlist.map((model) => model.trim().toLocaleLowerCase()).filter(Boolean));
    const blocklist = new Set(capability.modelBlocklist.map((model) => model.trim().toLocaleLowerCase()).filter(Boolean));
    for (const model of availableInEveryKey) {
      if (!allowlist.has(model) || blocklist.has(model)) {
        availableInEveryKey.delete(model);
      }
    }
  }

  const seen = new Set<string>();
  return firstAllowlist.flatMap((model) => {
    const trimmed = model.trim();
    const normalized = trimmed.toLocaleLowerCase();
    if (!trimmed || !availableInEveryKey.has(normalized) || seen.has(normalized)) return [];
    seen.add(normalized);
    return [trimmed];
  });
}

function NumberInput({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return <input className={inputClassName} inputMode="numeric" min={0} type="number" value={value} onChange={(event) => onChange(event.target.value)} />;
}

function Field({ label, children, className = "" }: { label: string; children: ReactNode; className?: string }) {
  return <label className={`grid gap-1.5 text-xs font-medium text-muted-foreground ${className}`}>{label}{children}</label>;
}
