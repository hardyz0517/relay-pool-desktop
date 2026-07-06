import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { ArrowLeft } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, IconButton, PageForm, SelectControl, SwitchControl } from "@/components/ui";
import type { ChannelMonitor, ChannelMonitorRequestTemplate, CreateChannelMonitorInput } from "@/lib/types/channelMonitors";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import {
  createEmptyMonitorDraft,
  draftToMonitorInput,
  formatTemplateLabel,
  monitorTemplateOptionsForProtocol,
  monitorToDraft,
  protocolForMonitorTemplate,
  targetTypeOptions,
  validateMonitorDraft,
  type ChannelMonitorProtocol,
  type ChannelMonitorDraft,
} from "./channelMonitorViewModel";

type ChannelMonitorFormProps = {
  open: boolean;
  monitor: ChannelMonitor | null;
  stations: Station[];
  keys: KeyPoolItem[];
  templates: ChannelMonitorRequestTemplate[];
  saving: boolean;
  onClose: () => void;
  onSubmit: (input: CreateChannelMonitorInput) => Promise<void> | void;
};

const inputClassName =
  "h-8 rounded-[8px] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:ring-2 focus:ring-teal-100";

const protocolOptions: Array<{
  value: ChannelMonitorProtocol;
  title: string;
  description: string;
}> = [
  {
    value: "chat_completions",
    title: "OpenAI Compatible",
    description: "使用 /v1/chat/completions，发送 messages；适合大多数兼容站。",
  },
  {
    value: "responses",
    title: "Responses API",
    description: "使用 /v1/responses，默认带 instructions + input；适合本站自检/Codex。",
  },
];

export function ChannelMonitorForm({
  open,
  monitor,
  stations,
  keys,
  templates,
  saving,
  onClose,
  onSubmit,
}: ChannelMonitorFormProps) {
  const [draft, setDraft] = useState<ChannelMonitorDraft>(() => createEmptyMonitorDraft(stations, templates));

  useEffect(() => {
    if (!open) {
      return;
    }
    setDraft(monitor ? monitorToDraft(monitor) : createEmptyMonitorDraft(stations, templates));
  }, [monitor, open, stations, templates]);

  const stationOptions = useMemo(
    () => stations.map((station) => ({ value: station.id, label: station.name })),
    [stations],
  );
  const stationKeys = useMemo(
    () => keys.filter((key) => key.stationId === draft.stationId),
    [draft.stationId, keys],
  );
  const keyOptions = useMemo(
    () =>
      stationKeys.map((key) => ({
        value: key.id,
        label: key.name,
        description: key.enabled ? key.modelScopeSummary || key.groupName || "全部模型" : "已停用",
        disabled: !key.enabled,
      })),
    [stationKeys],
  );
  const selectedProtocol = protocolForMonitorTemplate(draft.templateId, templates);
  const protocolTemplates = useMemo(
    () => monitorTemplateOptionsForProtocol(templates, selectedProtocol),
    [selectedProtocol, templates],
  );
  const templateOptions = useMemo(
    () =>
      protocolTemplates.map((template) => ({
        value: template.id,
        label: template.name,
        description: formatTemplateLabel(template),
        disabled: !template.enabled,
      })),
    [protocolTemplates],
  );
  const validationError = validateMonitorDraft(draft, { templates, keys });
  const canSubmit = !validationError && !saving;
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

  function handleProtocolChange(protocol: ChannelMonitorProtocol) {
    const nextTemplate = monitorTemplateOptionsForProtocol(templates, protocol)
      .find((template) => template.enabled);
    if (!nextTemplate) {
      return;
    }
    updateDraft({ templateId: nextTemplate.id });
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }
    await onSubmit(draftToMonitorInput(draft));
  }

  if (!open) {
    return null;
  }

  return (
    <div className="absolute -inset-[var(--shell-page-gap)] z-30 overflow-y-auto overflow-x-hidden bg-background p-[var(--shell-page-gap)]">
      <PageScaffold
        title={monitor ? "编辑渠道监控" : "新增渠道监控"}
        description="配置本地探测任务"
        stickyHeader
        backAction={
          <IconButton label="返回监控列表" onClick={onClose} disabled={saving}>
            <ArrowLeft className="h-4 w-4" />
          </IconButton>
        }
      >
        <PageForm
          id="channel-monitor-form"
          className="min-h-[520px]"
          onSubmit={handleSubmit}
          footer={
            <div className="flex w-full min-w-0 items-center justify-between gap-3">
              <div className="min-w-0 truncate text-xs text-rose-600">{validationError ?? ""}</div>
              <div className="flex shrink-0 justify-end gap-2">
                <Button variant="outline" onClick={onClose} disabled={saving}>
                  取消
                </Button>
                <Button type="submit" disabled={!canSubmit}>
                  {saving ? "保存中" : "保存"}
                </Button>
              </div>
            </div>
          }
        >
        <div className="grid gap-3 md:grid-cols-[minmax(0,1.3fr)_12rem_12rem]">
          <Field label="监控名称">
            <input
              className={inputClassName}
              value={draft.name}
              onChange={(event) => updateDraft({ name: event.target.value })}
            />
          </Field>
          <Field label="目标类型">
            <SelectControl
              ariaLabel="目标类型"
              className={inputClassName}
              value={draft.targetType}
              options={targetTypeOptions}
              onChange={handleTargetTypeChange}
            />
          </Field>
          <Field label="启用状态">
            <SwitchControl
              checked={draft.enabled}
              ariaLabel="启用监控"
              onCheckedChange={() => updateDraft({ enabled: !draft.enabled })}
              onLabel="启用"
              offLabel="停用"
              className="h-8"
            />
          </Field>
        </div>

        <div className="grid gap-3 md:grid-cols-2">
          <Field label="中转站">
            <SelectControl
              ariaLabel="中转站"
              className={inputClassName}
              value={draft.stationId}
              options={stationOptions}
              placeholder="请选择中转站"
              onChange={handleStationChange}
            />
          </Field>
          <Field label="站点密钥">
            <SelectControl
              ariaLabel="站点密钥"
              className={inputClassName}
              value={draft.stationKeyId}
              options={keyOptions}
              placeholder={isStationTarget ? "中转站目标不需要选择密钥" : "请选择密钥"}
              disabled={isStationTarget}
              onChange={(stationKeyId) => updateDraft({ stationKeyId })}
            />
          </Field>
        </div>

        <div className="grid gap-3 md:grid-cols-2">
          <Field label="OpenAI 协议" className="md:col-span-2">
            <div className="grid gap-2 rounded-[8px] border border-sky-100 bg-sky-50/35 p-2 md:grid-cols-2">
              {protocolOptions.map((option) => {
                const active = selectedProtocol === option.value;
                const disabled = !monitorTemplateOptionsForProtocol(templates, option.value).some((template) => template.enabled);
                return (
                  <button
                    key={option.value}
                    type="button"
                    className={`min-h-[64px] rounded-[8px] border bg-white px-3 py-2 text-left transition ${
                      active
                        ? "border-teal-500 text-teal-700 shadow-[0_0_0_1px_rgba(20,184,166,0.16)]"
                        : "border-border text-slate-600 hover:border-teal-200 hover:bg-teal-50/25"
                    } ${disabled ? "cursor-not-allowed opacity-50" : ""}`}
                    disabled={disabled}
                    onClick={() => handleProtocolChange(option.value)}
                  >
                    <div className="text-sm font-semibold">{option.title}</div>
                    <div className="mt-1 text-xs leading-5 text-muted-foreground">{option.description}</div>
                  </button>
                );
              })}
            </div>
          </Field>

          <Field label="请求模板">
            <SelectControl
              ariaLabel="请求模板"
              className={inputClassName}
              value={draft.templateId}
              options={templateOptions}
              placeholder="请选择模板"
              onChange={(templateId) => updateDraft({ templateId })}
            />
          </Field>
          <Field label="检测模型">
            <input
              className={inputClassName}
              value={draft.detectionModel}
              placeholder="例如 gpt-4o-mini"
              onChange={(event) => updateDraft({ detectionModel: event.target.value })}
            />
          </Field>
        </div>

        <div className="grid gap-3 md:grid-cols-5">
          <Field label="间隔（秒）">
            <NumberInput value={draft.intervalSeconds} onChange={(intervalSeconds) => updateDraft({ intervalSeconds })} />
          </Field>
          <Field label="抖动（秒）">
            <NumberInput value={draft.jitterSeconds} onChange={(jitterSeconds) => updateDraft({ jitterSeconds })} />
          </Field>
          <Field label="超时（秒）">
            <NumberInput value={draft.timeoutSeconds} onChange={(timeoutSeconds) => updateDraft({ timeoutSeconds })} />
          </Field>
          <Field label="最大并发">
            <NumberInput
              value={draft.maxConcurrency}
              disabled={!isStationTarget}
              onChange={(maxConcurrency) => updateDraft({ maxConcurrency })}
            />
          </Field>
          <Field label="失败阈值">
            <NumberInput
              value={draft.consecutiveFailureThreshold}
              onChange={(consecutiveFailureThreshold) => updateDraft({ consecutiveFailureThreshold })}
            />
          </Field>
        </div>

        <Field label="备注">
          <textarea
            className={`${inputClassName} min-h-20 resize-none py-2`}
            value={draft.note}
            onChange={(event) => updateDraft({ note: event.target.value })}
          />
        </Field>
        </PageForm>
      </PageScaffold>
    </div>
  );
}

function NumberInput({
  value,
  disabled,
  onChange,
}: {
  value: string;
  disabled?: boolean;
  onChange: (value: string) => void;
}) {
  return (
    <input
      className={inputClassName}
      disabled={disabled}
      inputMode="numeric"
      min={0}
      type="number"
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  );
}

function Field({ label, children, className = "" }: { label: string; children: ReactNode; className?: string }) {
  return (
    <label className={`grid gap-1.5 text-xs font-medium text-muted-foreground ${className}`}>
      {label}
      {children}
    </label>
  );
}
