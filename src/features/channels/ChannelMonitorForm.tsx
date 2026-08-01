import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { AlertTriangle, ArrowLeft, Check, Plus, RefreshCw, X } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, IconButton, PageForm, SectionCard, SelectControl, SwitchControl } from "@/components/ui";
import type {
  ChannelMonitor,
  ChannelMonitorHealthWritebackMode,
  ChannelMonitorProtocolKind,
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
import { profileLabel } from "@/lib/channelMonitorDisplay";
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
  { value: "disabled", label: "不写回", description: "只保留监控结果，不影响 Key 健康状态" },
  { value: "observe_only", label: "仅观察", description: "记录健康观察，但不改变路由资格" },
  { value: "authoritative", label: "权威写回", description: "达到阈值后更新 Key 健康状态，仅限标准 API Profile" },
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
  const selectedProfile = capabilities?.profiles.find((profile) => profile.id === draft.clientProfileId);
  const validationError = validateMonitorDraft(draft, { templates, keys, capabilities });
  const highRisk = draft.healthWritebackMode === "authoritative" || Number(draft.intervalSeconds) < 60;
  const canSubmit = !validationError && !saving && (!highRisk || riskAcknowledged);
  const isStationTarget = draft.targetType === "station";
  const theoreticalAttempts = (1 + draft.fallbackModels.filter((model) => model.trim()).length)
    * (Number(draft.retryMaxAttemptsPerModel) || 0);

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
                <input className={inputClassName} value={draft.primaryModel} placeholder="例如 gpt-4.1-mini" onChange={(event) => updateDraft({ primaryModel: event.target.value })} />
              </Field>
            </div>
            <FallbackModelEditor models={draft.fallbackModels} onChange={(fallbackModels) => updateDraft({ fallbackModels })} />
          </SectionCard>

          <SectionCard title="调度与预算">
            <div className="grid gap-3 md:grid-cols-5">
              <Field label="间隔（秒）"><NumberInput value={draft.intervalSeconds} onChange={(intervalSeconds) => updateDraft({ intervalSeconds })} /></Field>
              <Field label="抖动（秒）"><NumberInput value={draft.jitterSeconds} onChange={(jitterSeconds) => updateDraft({ jitterSeconds })} /></Field>
              <Field label="单次超时（毫秒）"><NumberInput value={draft.attemptTimeoutMs} onChange={(attemptTimeoutMs) => updateDraft({ attemptTimeoutMs })} /></Field>
              <Field label="任务超时（毫秒）"><NumberInput value={draft.executionTimeoutMs} onChange={(executionTimeoutMs) => updateDraft({ executionTimeoutMs })} /></Field>
              <Field label="每日尝试次数上限"><NumberInput value={draft.riskDailyProbeBudget} onChange={(riskDailyProbeBudget) => updateDraft({ riskDailyProbeBudget })} /></Field>
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
                <SelectControl ariaLabel="健康写回" className={inputClassName} value={draft.healthWritebackMode} options={healthWritebackOptions} onChange={(healthWritebackMode) => updateDraft({ healthWritebackMode })} />
              </Field>
              <Field label="连续失败阈值"><NumberInput value={draft.healthFailureThreshold} onChange={(healthFailureThreshold) => updateDraft({ healthFailureThreshold })} /></Field>
              <Field label="连续恢复阈值"><NumberInput value={draft.healthRecoveryThreshold} onChange={(healthRecoveryThreshold) => updateDraft({ healthRecoveryThreshold })} /></Field>
            </div>
            <div className="mt-3 rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
              当前配置单目标每次任务理论最多 {theoreticalAttempts} 次请求 · {profileLabel(draft.clientProfileId)} v{draft.clientProfileVersion}
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

function FallbackModelEditor({ models, onChange }: { models: string[]; onChange: (models: string[]) => void }) {
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
              <input className={`${inputClassName} min-w-0 flex-1`} value={model} placeholder={`回退模型 ${index + 1}`} onChange={(event) => onChange(models.map((item, itemIndex) => itemIndex === index ? event.target.value : item))} />
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

function NumberInput({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  return <input className={inputClassName} inputMode="numeric" min={0} type="number" value={value} onChange={(event) => onChange(event.target.value)} />;
}

function Field({ label, children, className = "" }: { label: string; children: ReactNode; className?: string }) {
  return <label className={`grid gap-1.5 text-xs font-medium text-muted-foreground ${className}`}>{label}{children}</label>;
}
