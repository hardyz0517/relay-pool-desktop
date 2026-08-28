import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Bell, BellOff, Check, MonitorDot, RotateCcw, Save, Trash2 } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Button, SectionCard, SelectControl, StatusBadge, SwitchControl, useToast } from "@/components/ui";
import {
  deleteAlertPolicy,
  getDesktopNotificationPermission,
  requestDesktopNotificationPermission,
  sendTestAlertNotification,
  updateAlertingSettings,
  upsertAlertPolicy,
} from "@/lib/api/alerting";
import { alertingWorkspaceQueryOptions } from "@/lib/queries/alertingQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import {
  ALERT_EVENT_OPTIONS,
  isAuditAlertEvent,
  DEFAULT_ALERTING_SETTINGS,
  defaultAlertPolicy,
  toAlertPolicyInput,
  toAlertingSettingsInput,
  type AlertPolicy,
  type AlertRepeatMode,
  type AlertingSettings,
  type AlertTriggerMode,
} from "@/lib/types/alerting";
import { readError } from "@/lib/errors";

const inputClassName =
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-control px-2.5 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30";

const fallbackPolicies = ALERT_EVENT_OPTIONS
  .filter((option) => option.configurable)
  .map((option) => defaultAlertPolicy(option.value));

type AlertingWorkspaceDraft = {
  settings: AlertingSettings;
  policies: AlertPolicy[];
};

function mergePoliciesWithDefaults(persistedPolicies: AlertPolicy[]): AlertPolicy[] {
  const eventPolicies = new Map(
    persistedPolicies
      .filter((policy) => policy.scopeKind === "event_type" && policy.eventType != null)
      .map((policy) => [policy.eventType, policy] as const),
  );
  const defaults = fallbackPolicies.map((policy) => eventPolicies.get(policy.eventType!) ?? policy);
  const scopedPolicies = persistedPolicies.filter(
    (policy) => policy.scopeKind !== "event_type" || policy.eventType == null,
  );
  return [...defaults, ...scopedPolicies];
}

/** Compact editor for global alerting controls and lifecycle policies. */
export function AlertingSettings() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const workspaceQuery = useActivityQuery(alertingWorkspaceQueryOptions());
  const [draft, setDraft] = useState<AlertingWorkspaceDraft>({
    settings: DEFAULT_ALERTING_SETTINGS,
    policies: fallbackPolicies,
  });
  const [selectedPolicyId, setSelectedPolicyId] = useState(fallbackPolicies[0].id);
  const [savingSettings, setSavingSettings] = useState(false);
  const [savingPolicy, setSavingPolicy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [desktopPermission, setDesktopPermission] = useState<"allowed" | "denied" | "unavailable">("unavailable");
  const [requestingPermission, setRequestingPermission] = useState(false);

  useEffect(() => {
    let active = true;
    void getDesktopNotificationPermission()
      .then((permission) => {
        if (active) setDesktopPermission(permission);
      })
      .catch(() => {
        if (active) setDesktopPermission("unavailable");
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    const workspace = workspaceQuery.data;
    if (!workspace) return;
    const policies = mergePoliciesWithDefaults(workspace.policies);
    setDraft({ settings: workspace.settings, policies });
    setSelectedPolicyId((current) => policies.some((policy) => policy.id === current) ? current : policies[0]?.id ?? "");
  }, [workspaceQuery.data]);

  const selectedPolicy = useMemo(
    () => draft.policies.find((policy) => policy.id === selectedPolicyId) ?? null,
    [draft.policies, selectedPolicyId],
  );
  const backendUnavailable = Boolean(workspaceQuery.error);
  const desktopNotificationsEnabled = desktopPermission === "allowed" && draft.settings.desktopEnabled;

  function patchSettings(patch: Partial<AlertingSettings>) {
    setDraft((current) => ({ ...current, settings: { ...current.settings, ...patch } }));
  }

  function patchPolicy(patch: Partial<AlertPolicy>) {
    if (!selectedPolicy) return;
    setDraft((current) => ({
      ...current,
      policies: current.policies.map((policy) =>
        policy.id === selectedPolicy.id ? { ...policy, ...patch } : policy,
      ),
    }));
  }

  function resetPolicyToDefaults() {
    if (!selectedPolicy?.eventType || selectedPolicy.scopeKind !== "event_type") return;
    const defaults = defaultAlertPolicy(selectedPolicy.eventType);
    patchPolicy({
      ...defaults,
      id: selectedPolicy.id,
      revision: selectedPolicy.revision,
      createdAtMs: selectedPolicy.createdAtMs,
      updatedAtMs: selectedPolicy.updatedAtMs,
    });
    toast.info("已恢复推荐默认值，请保存规则");
  }

  async function saveSettings() {
    setSavingSettings(true);
    setError(null);
    try {
      const saved = await updateAlertingSettings(toAlertingSettingsInput(draft.settings));
      setDraft((current) => ({ ...current, settings: saved }));
      queryClient.setQueryData(queryKeys.alertingWorkspace, (current: AlertingWorkspaceDraft | undefined) =>
        current ? { ...current, settings: saved } : current,
      );
      toast.success("提醒设置已保存");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("保存提醒设置失败", message);
    } finally {
      setSavingSettings(false);
    }
  }

  async function savePolicy() {
    if (!selectedPolicy) return;
    setSavingPolicy(true);
    setError(null);
    try {
      const persisted = workspaceQuery.data?.policies.some((policy) => policy.id === selectedPolicy.id) ?? false;
      const saved = await upsertAlertPolicy(
        toAlertPolicyInput(selectedPolicy, persisted ? selectedPolicy.revision : undefined),
      );
      setDraft((current) => ({
        ...current,
        policies: current.policies.map((policy) => policy.id === saved.id ? saved : policy),
      }));
      queryClient.invalidateQueries({ queryKey: queryKeys.alertingWorkspace });
      toast.success("告警规则已保存");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("保存告警规则失败", message);
    } finally {
      setSavingPolicy(false);
    }
  }

  async function removePolicy() {
    if (!selectedPolicy || selectedPolicy.id === "system_default" || selectedPolicy.id.startsWith("policy-")) {
      toast.info("系统默认规则不能删除");
      return;
    }
    try {
      await deleteAlertPolicy(selectedPolicy.id, selectedPolicy.revision);
      const remaining = draft.policies.filter((policy) => policy.id !== selectedPolicy.id);
      setDraft((current) => ({ ...current, policies: remaining }));
      setSelectedPolicyId(remaining[0]?.id ?? "");
      toast.success("告警规则已删除");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("删除告警规则失败", message);
    }
  }

  async function testNotification(channel: "in_app" | "desktop") {
    try {
      await sendTestAlertNotification(channel);
      toast.success(channel === "desktop" ? "桌面通知测试已发送" : "应用内通知测试已发送");
    } catch (requestError) {
      toast.error("通知测试失败", readError(requestError));
    }
  }

  async function requestDesktopPermission() {
    setRequestingPermission(true);
    try {
      const permission = await requestDesktopNotificationPermission();
      setDesktopPermission(permission);
      if (permission === "allowed") {
        toast.success("桌面通知权限已授予");
      } else {
        toast.error("未获得桌面通知权限");
      }
    } catch (requestError) {
      setDesktopPermission("denied");
      toast.error("申请桌面通知权限失败", readError(requestError));
    } finally {
      setRequestingPermission(false);
    }
  }

  return (
    <div className="grid min-w-0 gap-[var(--shell-page-gap)]">
      <SectionCard
        title="提醒与告警"
        description="为异常事实配置触发、恢复、重复提醒和通知渠道。恢复由事实恢复机制自动完成。"
        action={
          <StatusBadge tone={draft.settings.enabled && !draft.settings.paused ? "healthy" : "disabled"}>
            {draft.settings.enabled && !draft.settings.paused ? "已启用" : "已暂停"}
          </StatusBadge>
        }
      >
        <div className="grid gap-3">
          <SettingLine label="启用提醒">
            <SwitchControl
              ariaLabel="启用提醒"
              checked={draft.settings.enabled}
              disabled={savingSettings || backendUnavailable}
              onCheckedChange={() => patchSettings({ enabled: !draft.settings.enabled })}
              showLabel={false}
            />
          </SettingLine>
          <SettingLine label="全局暂停" description="暂停新的通知投递，不会关闭正在发生的事实型告警。">
            <SwitchControl
              ariaLabel="全局暂停"
              checked={draft.settings.paused}
              disabled={savingSettings || backendUnavailable}
              onCheckedChange={() => patchSettings({ paused: !draft.settings.paused })}
              showLabel={false}
            />
          </SettingLine>
          <SettingLine label="告警恢复后即删除" description="恢复确认后立即删除对应告警；后台会自动清理意外残留的已恢复记录。">
            <SwitchControl
              ariaLabel="告警恢复后即删除"
              checked={draft.settings.deleteResolvedIncidents}
              disabled={savingSettings || backendUnavailable}
              onCheckedChange={() => patchSettings({ deleteResolvedIncidents: !draft.settings.deleteResolvedIncidents })}
              showLabel={false}
            />
          </SettingLine>
          <SettingLine className="justify-start" label="免打扰时间" description="应用内记录仍会保留；严重告警可由规则选择绕过。">
            <div className="flex flex-wrap items-center justify-end gap-1.5">
              <SwitchControl
                ariaLabel="启用免打扰时间"
                checked={draft.settings.quietHoursEnabled}
                disabled={savingSettings || backendUnavailable}
                onCheckedChange={() => patchSettings({ quietHoursEnabled: !draft.settings.quietHoursEnabled })}
                showLabel={false}
              />
              <input aria-label="免打扰开始时间" className={inputClassName + " w-[92px]"} disabled={savingSettings || backendUnavailable} type="time" value={draft.settings.quietHoursStart} onChange={(event) => patchSettings({ quietHoursStart: event.target.value })} />
              <span className="text-xs text-muted-foreground">至</span>
              <input aria-label="免打扰结束时间" className={inputClassName + " w-[92px]"} disabled={savingSettings || backendUnavailable} type="time" value={draft.settings.quietHoursEnd} onChange={(event) => patchSettings({ quietHoursEnd: event.target.value })} />
            </div>
          </SettingLine>
          <SettingLine
            label="桌面通知"
            description="关闭后本应用不再投递桌面通知；系统授权状态不会被更改。"
          >
            <div className="flex flex-wrap items-center justify-end gap-1.5">
              <StatusBadge tone={desktopNotificationsEnabled ? "healthy" : desktopPermission === "denied" ? "warning" : "disabled"}>
                {desktopPermission === "allowed" ? desktopNotificationsEnabled ? "已启用" : "未启用" : permissionLabel(desktopPermission)}
              </StatusBadge>
              <SwitchControl
                ariaLabel="启用桌面通知"
                checked={draft.settings.desktopEnabled}
                disabled={savingSettings || backendUnavailable || desktopPermission !== "allowed"}
                onCheckedChange={() => patchSettings({ desktopEnabled: !draft.settings.desktopEnabled })}
                showLabel={false}
              />
              {desktopPermission !== "allowed" ? (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={requestingPermission || backendUnavailable}
                  onClick={() => void requestDesktopPermission()}
                >
                  <MonitorDot className="h-4 w-4" />
                  请求系统权限
                </Button>
              ) : null}
            </div>
          </SettingLine>
          <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border pt-3">
            <div className="flex flex-wrap items-center gap-2">
            <Button disabled={backendUnavailable} variant="outline" onClick={() => void testNotification("in_app")}>
              <Bell className="h-4 w-4" />测试应用内
            </Button>
            <Button disabled={backendUnavailable} variant="outline" onClick={() => void testNotification("desktop")}>
              <MonitorDot className="h-4 w-4" />测试桌面通知
            </Button>
            </div>
            <Button disabled={savingSettings || backendUnavailable} variant="primary" onClick={() => void saveSettings()}>
              <Save className="h-4 w-4" />保存设置
            </Button>
          </div>
        </div>
      </SectionCard>

      <SectionCard
        title="告警规则"
        description="每条规则独立控制出现几次、持续多久、何时恢复以及重复通知频率。"
      >
        {backendUnavailable ? (
          <div className="mb-3 flex items-start gap-2 rounded-[var(--surface-radius)] border border-warning-border bg-warning-surface px-3 py-2 text-xs text-warning-foreground">
            <BellOff className="mt-0.5 h-4 w-4 shrink-0" />
            <span>当前运行时尚未提供提醒策略接口，下面显示可编辑的默认模板；保存将在桌面接口启用后生效。</span>
          </div>
        ) : null}
        <div className="grid min-w-0 gap-4 lg:grid-cols-[minmax(190px,0.35fr)_minmax(0,1fr)]">
          <div className="h-[420px] min-h-0 overflow-y-auto overscroll-contain pr-1">
            <div className="grid content-start gap-1">
              {draft.policies.map((policy) => {
                const event = ALERT_EVENT_OPTIONS.find((option) => option.value === policy.eventType);
                const active = policy.id === selectedPolicyId;
                return (
                  <button
                    key={policy.id}
                    type="button"
                    className={`grid min-w-0 gap-1 rounded-[var(--surface-radius)] border px-3 py-2 text-left transition ${active ? "border-ring bg-selected" : "border-border bg-control hover:bg-hover"}`}
                    onClick={() => setSelectedPolicyId(policy.id)}
                  >
                    <span className="flex min-w-0 items-center justify-between gap-2">
                      <span className="truncate text-sm font-medium text-foreground">{policy.name}</span>
                      <span className={`h-2 w-2 shrink-0 rounded-full ${policy.enabled ? "bg-success-solid" : "bg-muted"}`} />
                    </span>
                    <span className="truncate text-xs text-muted-foreground">{event?.label ?? policy.eventType ?? "全局"}</span>
                  </button>
                );
              })}
            </div>
          </div>
          {selectedPolicy ? (
            <PolicyEditor
              policy={selectedPolicy}
              disabled={savingPolicy || backendUnavailable}
              onChange={patchPolicy}
              onDelete={() => void removePolicy()}
              onReset={resetPolicyToDefaults}
              onSave={() => void savePolicy()}
            />
          ) : (
            <div className="grid min-h-32 place-items-center rounded-[var(--surface-radius)] border border-dashed border-border text-sm text-muted-foreground">请选择或新增一条规则</div>
          )}
        </div>
        {error ? <div className="mt-3 text-xs text-danger-foreground">{error}</div> : null}
      </SectionCard>
    </div>
  );
}

function PolicyEditor({
  policy,
  disabled,
  onChange,
  onDelete,
  onReset,
  onSave,
}: {
  policy: AlertPolicy;
  disabled: boolean;
  onChange: (patch: Partial<AlertPolicy>) => void;
  onDelete: () => void;
  onReset: () => void;
  onSave: () => void;
}) {
  const eventOptions = ALERT_EVENT_OPTIONS.map((option) => ({ value: option.value, label: option.label, description: option.description }));
  const triggerOptions = [
    { value: "immediate", label: "立即触发" },
    { value: "consecutive_occurrences", label: "连续出现次数" },
    { value: "active_duration", label: "持续时间" },
  ];
  const recoveryOptions = [
    { value: "consecutive_healthy", label: "连续健康次数" },
    { value: "healthy_duration", label: "健康持续时间" },
  ];
  const repeatOptions = [
    { value: "never", label: "不重复" },
    { value: "interval", label: "按时间间隔" },
    { value: "severity_escalation", label: "仅严重度升级" },
    { value: "interval_and_escalation", label: "间隔或严重度升级" },
  ];
  const auditEvent = isAuditAlertEvent(policy.eventType);
  return (
    <div className="grid min-w-0 gap-3 rounded-[var(--surface-radius)] border border-border bg-control p-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <EditorRow className="w-full max-w-[360px]" label="监测名称">
          <input aria-label="规则名称" className={inputClassName + " font-medium"} disabled={disabled} value={policy.name} onChange={(event) => onChange({ name: event.target.value })} />
        </EditorRow>
        <SwitchControl ariaLabel="启用当前规则" checked={policy.enabled} disabled={disabled} onCheckedChange={() => onChange({ enabled: !policy.enabled })} />
      </div>
      <EditorRow label="监测事件">
        <SelectControl ariaLabel="监测事件" className={inputClassName} disabled={disabled} options={eventOptions} value={policy.eventType ?? "collector_failed"} onChange={(value) => onChange({ eventType: value as AlertPolicy["eventType"] })} />
      </EditorRow>
      <div className="grid gap-3 sm:grid-cols-2">
        <div className="grid content-start gap-2">
          <EditorRow label="触发条件">
            <SelectControl ariaLabel="触发条件" className={inputClassName} disabled={disabled} options={triggerOptions} value={policy.triggerMode} onChange={(value) => applyTriggerMode(value as AlertTriggerMode, policy.eventType, onChange)} />
          </EditorRow>
          {policy.triggerMode === "consecutive_occurrences" ? <NumberField ariaLabel="触发次数" label="出现次数" min={1} max={100} value={policy.triggerCount ?? 2} disabled={disabled} onChange={(value) => onChange({ triggerCount: value, triggerDurationSeconds: null })} /> : null}
          {policy.triggerMode === "active_duration" ? <NumberField ariaLabel="触发持续分钟" label="持续分钟" min={1} max={43_200} value={Math.max(1, Math.round((policy.triggerDurationSeconds ?? 300) / 60))} disabled={disabled} onChange={(value) => onChange({ triggerDurationSeconds: value * 60, triggerCount: null })} /> : null}
        </div>
        <div className="grid content-start gap-2">
          {auditEvent ? (
            <div className="flex min-h-8 items-center justify-between gap-3">
              <span className="text-xs text-muted-foreground">恢复条件</span>
              <span className="text-xs text-muted-foreground">不适用（变更事件只记录一次）</span>
            </div>
          ) : (
            <>
              <EditorRow label="恢复条件">
                <SelectControl ariaLabel="恢复条件" className={inputClassName} disabled={disabled} options={recoveryOptions} value={policy.recoveryMode} onChange={(value) => onChange(value === "healthy_duration" ? { recoveryMode: "healthy_duration", recoveryCount: null, recoveryDurationSeconds: 300 } : { recoveryMode: "consecutive_healthy", recoveryCount: 1, recoveryDurationSeconds: null })} />
              </EditorRow>
              {policy.recoveryMode === "consecutive_healthy" ? <NumberField ariaLabel="恢复健康次数" label="健康次数" min={1} max={100} value={policy.recoveryCount ?? 1} disabled={disabled} onChange={(value) => onChange({ recoveryCount: value, recoveryDurationSeconds: null })} /> : null}
              {policy.recoveryMode === "healthy_duration" ? <NumberField ariaLabel="恢复持续分钟" label="健康分钟" min={1} max={43_200} value={Math.max(1, Math.round((policy.recoveryDurationSeconds ?? 300) / 60))} disabled={disabled} onChange={(value) => onChange({ recoveryDurationSeconds: value * 60, recoveryCount: null })} /> : null}
            </>
          )}
        </div>
      </div>
      <div className="grid gap-2 border-t border-border pt-3 sm:grid-cols-2">
        <EditorRow label="最低严重度">
          <SelectControl ariaLabel="最低严重度" className={inputClassName} disabled={disabled} options={[{ value: "", label: "不限制" }, { value: "info", label: "信息" }, { value: "warning", label: "警告" }, { value: "critical", label: "严重" }]} value={policy.minimumSeverity ?? ""} onChange={(value) => onChange({ minimumSeverity: value ? value as AlertPolicy["minimumSeverity"] : null })} />
        </EditorRow>
        <EditorRow label="重复通知">
          <SelectControl ariaLabel="重复通知" className={inputClassName} disabled={disabled} options={repeatOptions} value={policy.repeatMode} onChange={(value) => onChange({ repeatMode: value as AlertRepeatMode, repeatIntervalSeconds: value === "interval" || value === "interval_and_escalation" ? (policy.repeatIntervalSeconds ?? 3600) : null })} />
        </EditorRow>
        {policy.repeatMode === "interval" || policy.repeatMode === "interval_and_escalation" ? <NumberField ariaLabel="重复间隔分钟" label="重复间隔分钟" min={1} max={43_200} value={Math.max(1, Math.round((policy.repeatIntervalSeconds ?? 3600) / 60))} disabled={disabled} onChange={(value) => onChange({ repeatIntervalSeconds: value * 60 })} /> : null}
        <NumberField ariaLabel="冷却分钟" label="冷却分钟" min={0} max={43_200} value={Math.max(0, Math.round(policy.cooldownSeconds / 60))} disabled={disabled} onChange={(value) => onChange({ cooldownSeconds: value * 60 })} />
      </div>
      <div className="grid gap-2 border-t border-border pt-3 sm:grid-cols-2">
        <SettingLine label="应用内通知"><SwitchControl ariaLabel="应用内通知" checked={policy.inAppEnabled} disabled={disabled} onCheckedChange={() => onChange({ inAppEnabled: !policy.inAppEnabled })} showLabel={false} /></SettingLine>
        <SettingLine label="桌面通知"><SwitchControl ariaLabel="桌面通知" checked={policy.desktopEnabled} disabled={disabled} onCheckedChange={() => onChange({ desktopEnabled: !policy.desktopEnabled })} showLabel={false} /></SettingLine>
        <SettingLine label="恢复时通知"><SwitchControl ariaLabel="恢复时通知" checked={policy.recoveryNotificationEnabled} disabled={disabled} onCheckedChange={() => onChange({ recoveryNotificationEnabled: !policy.recoveryNotificationEnabled })} showLabel={false} /></SettingLine>
      </div>
      <div className="flex flex-wrap justify-end gap-2 border-t border-border pt-3">
        <Button disabled={disabled || policy.id === "system_default" || policy.id.startsWith("policy-")} size="sm" variant="danger" onClick={onDelete}><Trash2 className="h-4 w-4" />删除</Button>
        {policy.scopeKind === "event_type" && policy.eventType != null ? <Button disabled={disabled} size="sm" variant="outline" onClick={onReset}><RotateCcw className="h-4 w-4" />重置为默认</Button> : null}
        <Button disabled={disabled} size="sm" variant="primary" onClick={onSave}><Check className="h-4 w-4" />保存规则</Button>
      </div>
    </div>
  );
}

function applyTriggerMode(mode: AlertTriggerMode, eventType: AlertPolicy["eventType"], onChange: (patch: Partial<AlertPolicy>) => void) {
  if (mode === "immediate") onChange({ triggerMode: mode, triggerCount: null, triggerDurationSeconds: null });
  else if (mode === "active_duration") onChange({ triggerMode: mode, triggerCount: null, triggerDurationSeconds: 300 });
  else onChange({ triggerMode: mode, triggerCount: eventType === "collector_failed" ? 3 : 2, triggerDurationSeconds: null });
}

function permissionLabel(value: "allowed" | "denied" | "unavailable") {
  return ({ allowed: "已允许", denied: "已拒绝", unavailable: "不可用" } as const)[value];
}

function SettingLine({ label, description, children, className = "" }: { label: string; description?: string; children: ReactNode; className?: string }) {
  return <div className={`flex min-h-8 items-center justify-between gap-3 ${className}`}><div className="min-w-0"><div className="text-sm text-foreground">{label}</div>{description ? <div className="text-xs text-muted-foreground">{description}</div> : null}</div><div className="shrink-0">{children}</div></div>;
}

function EditorRow({ label, children, className = "" }: { label: string; children: ReactNode; className?: string }) {
  return <label className={`grid min-w-0 items-center gap-1.5 sm:grid-cols-[6rem_minmax(0,1fr)] ${className}`}><span className="text-xs text-muted-foreground">{label}</span>{children}</label>;
}

function NumberField({ ariaLabel, label, min, max, value, disabled, onChange }: { ariaLabel: string; label: string; min: number; max: number; value: number; disabled: boolean; onChange: (value: number) => void }) {
  return <EditorRow label={label}><input aria-label={ariaLabel} className={inputClassName} disabled={disabled} max={max} min={min} type="number" value={value} onChange={(event) => onChange(Number(event.target.value) || min)} /></EditorRow>;
}
