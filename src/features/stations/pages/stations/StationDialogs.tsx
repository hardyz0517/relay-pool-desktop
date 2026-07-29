import type { FormEvent, ReactNode } from "react";
import { ShieldCheck } from "lucide-react";
import { Button, Dialog, MaskedSecret, PropertyList, PropertyRow, SelectControl, StatusBadge } from "@/components/ui";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import { stationKeyStatusLabels, type StationCredentials, type StationKey, type StationKeyStatus } from "@/lib/types/stationKeys";
import { stationStatusLabels, stationTypeLabels, stationTypeOptions, type Station } from "@/lib/types/stations";
import {
  stationEndpointOriginWarnings,
  type StationFormState,
  type StationKeyFormState,
} from "./formModel";
import {
  collectorRunStatusLabel,
  collectorTaskTypeLabel,
  formatMultiplier,
  formatNullableTime,
  groupBindingStatusLabel,
} from "./displayModel";

export type DialogMode = "create" | "edit" | "detail" | null;

const statusTone: Record<Station["status"], "healthy" | "warning" | "error" | "disabled" | "info"> = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
};

const inputClassName = "h-8 rounded-[12px] border border-info-border bg-info-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:bg-surface focus:ring-2 focus:ring-ring/20";

export function StationDialogs({
  activeDialogStation,
  actionSaving,
  credentials,
  dialogMode,
  form,
  keyDialogOpen,
  keyForm,
  onChange,
  onClose,
  onKeyDialogOpenChange,
  onKeyFormChange,
  onKeySave,
  onRemoveLoginInfo,
  onSubmit,
  saving,
}: {
  activeDialogStation: Station | null;
  actionSaving: boolean;
  credentials: StationCredentials | null;
  dialogMode: DialogMode;
  form: StationFormState;
  keyDialogOpen: boolean;
  keyForm: StationKeyFormState;
  onChange: (nextForm: StationFormState) => void;
  onClose: () => void;
  onKeyDialogOpenChange: (next: boolean) => void;
  onKeyFormChange: (next: StationKeyFormState) => void;
  onKeySave: (event: FormEvent<HTMLFormElement>) => void;
  onRemoveLoginInfo: () => Promise<void>;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
  saving: boolean;
}) {
  if (dialogMode === "detail" && activeDialogStation) {
    return (
      <>
        {keyDialogOpen && (
          <KeyDialog
            actionSaving={actionSaving}
            keyForm={keyForm}
            onKeyDialogOpenChange={onKeyDialogOpenChange}
            onKeyFormChange={onKeyFormChange}
            onKeySave={onKeySave}
          />
        )}
      </>
    );
  }

  const endpointOriginWarnings =
    dialogMode === "edit" && activeDialogStation
      ? stationEndpointOriginWarnings(activeDialogStation, form)
      : [];

  return (
    <>
      <Dialog
        open
        title={dialogMode === "edit" ? "编辑站点" : "新增站点"}
        description={dialogMode === "edit" ? "密钥留空则保留旧值。登录账号区用于采集。": undefined}
        onClose={onClose}
        footer={
          <div className="flex justify-end gap-2">
            <Button variant="outline" onClick={onClose}>取消</Button>
            <Button disabled={saving} type="submit" form="station-form">保存</Button>
          </div>
        }
      >
        <form id="station-form" className="grid gap-4 p-5" onSubmit={onSubmit}>
          <div className="grid gap-3 md:grid-cols-2">
            <Field label="站点名称">
              <input className={inputClassName} value={form.name} onChange={(event) => onChange({ ...form, name: event.target.value })} required />
            </Field>
            <Field label="站点类型">
              <SelectControl
                ariaLabel="站点类型"
                className={inputClassName}
                value={form.stationType}
                options={stationTypeOptions}
                onChange={(stationType) => onChange({ ...form, stationType })}
              />
            </Field>
          </div>
          <div className="grid gap-3 md:grid-cols-2">
            <Field label="前端网址">
              <input className={inputClassName} value={form.websiteUrl} onChange={(event) => onChange({ ...form, websiteUrl: event.target.value })} placeholder="https://example.com" required />
            </Field>
            <Field label="API Base URL">
              <input className={inputClassName} value={form.apiBaseUrl} onChange={(event) => onChange({ ...form, apiBaseUrl: event.target.value })} placeholder="https://api.example.com/v1" required />
            </Field>
          </div>
          <div className="flex justify-end">
            <Button variant="outline" onClick={() => onChange({ ...form, apiBaseUrl: form.websiteUrl })}>
              复制前端网址
            </Button>
          </div>
          {endpointOriginWarnings.length > 0 && (
            <div className="rounded-[var(--surface-radius)] border border-warning-border bg-warning-surface px-3 py-2 text-xs text-warning-foreground">
              {endpointOriginWarnings.map((warning) => (
                <div key={warning}>{warning}</div>
              ))}
            </div>
          )}
          <Field label={dialogMode === "edit" ? "密钥（留空保留旧值）" : "密钥"}>
            <input className={inputClassName} value={form.apiKey} onChange={(event) => onChange({ ...form, apiKey: event.target.value })} placeholder={dialogMode === "edit" ? "留空保留旧密钥" : "sk-..."} required={dialogMode !== "edit"} />
          </Field>
          <div className="grid gap-3 md:grid-cols-3">
            <Field label="兑换比例">
              <input className={inputClassName} min="0.01" step="0.01" type="number" value={form.creditPerCny} onChange={(event) => onChange({ ...form, creditPerCny: event.target.value })} />
            </Field>
            <Field label="低余额阈值">
              <input className={inputClassName} min="0" step="0.01" type="number" value={form.lowBalanceThresholdCny} onChange={(event) => onChange({ ...form, lowBalanceThresholdCny: event.target.value })} placeholder="使用全局设置" />
            </Field>
            <Field label="采集频率 分钟">
              <input className={inputClassName} min="1" step="1" type="number" value={form.collectionIntervalMinutes} onChange={(event) => onChange({ ...form, collectionIntervalMinutes: event.target.value })} placeholder="5" />
            </Field>
          </div>
          <div className="grid gap-3 md:grid-cols-3">
            <label className="flex items-end gap-2 pb-2 text-sm text-foreground">
              <input checked={form.enabled} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onChange({ ...form, enabled: event.target.checked })} />
              启用站点
            </label>
          </div>
          <Field label="备注">
            <textarea className={`${inputClassName} min-h-20 resize-none py-2`} value={form.note} onChange={(event) => onChange({ ...form, note: event.target.value })} />
          </Field>
          <SectionBlock title="登录账号（用于采集）">
            <div className="grid gap-3 md:grid-cols-2">
              <Field label="登录用户名 / 邮箱">
                <input className={inputClassName} value={form.loginUsername} onChange={(event) => onChange({ ...form, loginUsername: event.target.value })} placeholder="user@example.com" />
              </Field>
              <Field label="登录密码">
                <input className={inputClassName} type="password" value={form.loginPassword} onChange={(event) => onChange({ ...form, loginPassword: event.target.value })} placeholder="留空保留旧密码" />
              </Field>
            </div>
            <div className="mt-2 flex items-center gap-4 text-sm text-foreground">
              <label className="flex items-center gap-2">
                <input checked={form.rememberPassword} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onChange({ ...form, rememberPassword: event.target.checked })} />
                记住密码
              </label>
              <span className="text-xs text-muted-foreground">保存后密码会写入本地加密存储；留空不会覆盖旧密码。</span>
            </div>
            {credentials && (
              <div className="mt-3 rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-xs text-foreground shadow-[var(--surface-shadow)]">
                当前登录状态: {credentials.loginStatus}
                {credentials.loginError ? ` · ${credentials.loginError}` : ""}
              </div>
            )}
            {credentials && (
              <div className="mt-3 flex justify-end">
                <Button variant="outline" onClick={onRemoveLoginInfo} disabled={actionSaving}>清除登录信息</Button>
              </div>
            )}
          </SectionBlock>
        </form>
      </Dialog>

      {keyDialogOpen && (
        <KeyDialog
          actionSaving={actionSaving}
          keyForm={keyForm}
          onKeyDialogOpenChange={onKeyDialogOpenChange}
          onKeyFormChange={onKeyFormChange}
          onKeySave={onKeySave}
        />
      )}
    </>
  );
}

export function DetailBody({
  activeDialogStation,
  changeEvents,
  credentials,
  keyCountLabel,
  snapshot,
  snapshots,
  stationKeys,
  groupBindings,
  rateRecords,
  collectorRuns,
  onDeleteKey,
  onEditKey,
}: {
  activeDialogStation: Station;
  changeEvents: ChangeEvent[];
  credentials: StationCredentials | null;
  keyCountLabel: string;
  snapshot: CollectorSnapshot | null;
  snapshots: CollectorSnapshot[];
  stationKeys: StationKey[];
  groupBindings: StationGroupBinding[];
  rateRecords: GroupRateRecord[];
  collectorRuns: CollectorRun[];
  onDeleteKey: (key: StationKey) => void;
  onEditKey: (key: StationKey) => void;
}) {
  return (
    <div className="space-y-4 p-5">
      <PropertyList className="overflow-hidden rounded-[var(--surface-radius)] border border-info-border bg-surface/80">
        <PropertyRow label="站点名称" value={activeDialogStation.name} />
        <PropertyRow label="站点类型" value={stationTypeLabels[activeDialogStation.stationType]} />
        <PropertyRow label="前端网址" value={<code className="text-xs">{activeDialogStation.websiteUrl}</code>} />
        <PropertyRow label="API Base URL" value={<code className="text-xs">{activeDialogStation.apiBaseUrl}</code>} />
        <PropertyRow label="余额" value={activeDialogStation.balanceCny === null ? "未采集" : `¥${activeDialogStation.balanceCny.toFixed(2)}`} />
        <PropertyRow label="密钥数量" value={keyCountLabel} />
        <PropertyRow label="状态" value={stationStatusLabels[activeDialogStation.status]} />
        <PropertyRow label="采集时间" value={activeDialogStation.lastPricingFetchedAt ?? "未采集"} />
        <PropertyRow label="刷新时间" value={activeDialogStation.lastCheckedAt ?? "未检测"} />
      </PropertyList>

      <SectionBlock title="登录账号">
        {credentials ? (
          <PropertyList className="overflow-hidden rounded-[var(--surface-radius)] border border-info-border bg-surface/80">
            <PropertyRow label="登录用户名" value={credentials.loginUsername || "未设置"} />
            <PropertyRow label="密码" value={credentials.passwordPresent ? "已保存" : "未保存"} />
            <PropertyRow label="记住密码" value={credentials.rememberPassword ? "是" : "否"} />
            <PropertyRow label="登录状态" value={credentials.loginStatus} />
            <PropertyRow label="最近登录" value={credentials.lastLoginAt ?? "未登录"} />
            <PropertyRow label="登录错误" value={credentials.loginError ?? "无"} />
          </PropertyList>
        ) : (
          <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">未保存登录账号。</div>
        )}
      </SectionBlock>

      <SectionBlock title="密钥">
        <div className="space-y-2">
          {stationKeys.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无密钥。</div>
          ) : (
            stationKeys.map((key) => (
              <div key={key.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2.5 shadow-[var(--surface-shadow)]">
                <div className="min-w-0">
                  <div className="flex items-center gap-2">
                    <div className="truncate text-sm font-medium text-foreground">{key.name}</div>
                    <StatusBadge tone={statusTone[key.status]}>{stationKeyStatusLabels[key.status]}</StatusBadge>
                    <span className="text-[11px] text-muted-foreground">P{key.priority}</span>
                  </div>
                  <div className="mt-1 flex flex-wrap gap-2 text-xs text-muted-foreground">
                    <MaskedSecret value={key.apiKeyMasked} present={key.apiKeyPresent} />
                    <span>{key.groupName ?? "未分组"}</span>
                    <span>{key.tierLabel ?? "无档位"}</span>
                    <span>{key.enabled ? "启用" : "停用"}</span>
                  </div>
                </div>
                <div className="flex gap-2">
                  <Button variant="outline" onClick={() => onEditKey(key)}>编辑</Button>
                  <Button variant="danger" onClick={() => onDeleteKey(key)}>删除</Button>
                </div>
              </div>
            ))
          )}
        </div>
      </SectionBlock>

      <SectionBlock title="分组绑定">
        <div className="space-y-2">
          {groupBindings.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无分组绑定事实。</div>
          ) : (
            groupBindings.map((binding) => (
              <div
                key={binding.id}
                className="grid grid-cols-[minmax(0,1fr)_5rem_6rem_7rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2 text-xs shadow-[var(--surface-shadow)]"
              >
                <span className="truncate font-medium text-foreground">{binding.groupName}</span>
                <span>{formatMultiplier(binding.effectiveRateMultiplier ?? binding.defaultRateMultiplier)}</span>
                <StatusBadge tone={binding.bindingStatus === "missing" ? "warning" : "info"}>
                  {groupBindingStatusLabel(binding.bindingStatus)}
                </StatusBadge>
                <span className="truncate text-muted-foreground">{binding.rateSource ?? "未知"}</span>
              </div>
            ))
          )}
        </div>
      </SectionBlock>

      <SectionBlock title="倍率历史">
        <div className="space-y-2">
          {rateRecords.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无倍率历史。</div>
          ) : (
            rateRecords.slice(0, 8).map((record) => (
              <div
                key={record.id}
                className="grid grid-cols-[minmax(0,1fr)_5rem_7rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2 text-xs shadow-[var(--surface-shadow)]"
              >
                <span className="truncate font-medium text-foreground">{record.groupName}</span>
                <span>{formatMultiplier(record.effectiveRateMultiplier)}</span>
                <span className="truncate text-muted-foreground">{formatNullableTime(record.checkedAt)}</span>
              </div>
            ))
          )}
        </div>
      </SectionBlock>

      <SectionBlock title="采集任务">
        <div className="space-y-2">
          {collectorRuns.length === 0 ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无采集任务。</div>
          ) : (
            collectorRuns.slice(0, 8).map((run) => (
              <div
                key={run.id}
                className="grid grid-cols-[5rem_6rem_minmax(0,1fr)_5rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-surface px-3 py-2 text-xs shadow-[var(--surface-shadow)]"
              >
                <span className="font-medium text-foreground">{collectorTaskTypeLabel(run.taskType)}</span>
                <StatusBadge tone={run.status === "success" ? "healthy" : run.status === "failed" ? "error" : run.status === "manual_required" ? "warning" : "info"}>
                  {collectorRunStatusLabel(run.status)}
                </StatusBadge>
                <span className="truncate text-muted-foreground">{run.errorMessage ?? `${run.successCount}/${run.endpointCount} 接口`}</span>
                <span className="text-right text-muted-foreground">{run.durationMs == null ? "-" : `${run.durationMs}ms`}</span>
              </div>
            ))
          )}
        </div>
      </SectionBlock>

      <SectionBlock title="最新采集快照">
        {snapshot ? (
          <div className="space-y-2 rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm shadow-[var(--surface-shadow)]">
            <PropertyList>
              <PropertyRow label="来源" value={snapshot.source} />
              <PropertyRow label="状态" value={collectorRunStatusLabel(snapshot.status)} />
              <PropertyRow label="采集时间" value={snapshot.fetchedAt} />
              <PropertyRow label="错误" value={snapshot.errorMessage ?? "无"} />
            </PropertyList>
            <pre className="max-h-40 overflow-auto rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-[11px] text-muted-foreground">{JSON.stringify(snapshot.summaryJson, null, 2)}</pre>
            <div className="text-xs text-muted-foreground">历史快照：{snapshots.length} 条</div>
          </div>
        ) : (
          <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无快照。</div>
        )}
      </SectionBlock>

      <SectionBlock title="关联变更">
        {changeEvents.length === 0 ? (
          <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">暂无关联变更。</div>
        ) : (
          <div className="space-y-2">
            {changeEvents.slice(0, 6).map((event) => (
              <div key={event.id} className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-sm shadow-[var(--surface-shadow)]">
                <div className="flex items-center justify-between gap-2">
                  <span className="font-medium text-foreground">{event.title}</span>
                  <StatusBadge tone={event.severity === "critical" ? "error" : event.severity === "warning" ? "warning" : "info"}>
                    {event.severity === "critical" ? "严重" : event.severity === "warning" ? "警告" : "信息"}
                  </StatusBadge>
                </div>
                <div className="mt-1 text-xs text-muted-foreground">{event.message}</div>
              </div>
            ))}
          </div>
        )}
      </SectionBlock>

      <div className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 text-xs leading-5 text-foreground shadow-[var(--surface-shadow)]">
        登录账号用于信息采集；保存的密码会加密存储，采集快照和使用记录会统一脱敏。
      </div>
    </div>
  );
}

function SectionBlock({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-[var(--surface-radius)] border border-border bg-surface p-3 shadow-[var(--surface-shadow)]">
      <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-foreground">
        <ShieldCheck className="h-4 w-4 text-primary" />
        {title}
      </div>
      {children}
    </section>
  );
}

function KeyDialog({
  actionSaving,
  keyForm,
  onKeyDialogOpenChange,
  onKeyFormChange,
  onKeySave,
}: {
  actionSaving: boolean;
  keyForm: StationKeyFormState;
  onKeyDialogOpenChange: (next: boolean) => void;
  onKeyFormChange: (next: StationKeyFormState) => void;
  onKeySave: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <Dialog
      open
      title={keyForm.id ? "编辑密钥" : "新增密钥"}
      onClose={() => onKeyDialogOpenChange(false)}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={() => onKeyDialogOpenChange(false)}>取消</Button>
          <Button type="submit" form="station-key-form" disabled={actionSaving}>{actionSaving ? "保存中" : "保存"}</Button>
        </div>
      }
    >
      <form id="station-key-form" className="grid gap-4 p-5" onSubmit={onKeySave}>
        <div className="grid gap-3 md:grid-cols-2">
          <Field label="名称">
            <input className={inputClassName} value={keyForm.name} onChange={(event) => onKeyFormChange({ ...keyForm, name: event.target.value })} required />
          </Field>
          <Field label="优先级">
            <input className={inputClassName} type="number" value={keyForm.priority} onChange={(event) => onKeyFormChange({ ...keyForm, priority: event.target.value })} />
          </Field>
        </div>
        <Field label="密钥">
          <input className={inputClassName} value={keyForm.apiKey} onChange={(event) => onKeyFormChange({ ...keyForm, apiKey: event.target.value })} placeholder={keyForm.id ? "留空保留旧密钥" : "sk-..."} required={!keyForm.id} />
        </Field>
        <div className="grid gap-3 md:grid-cols-3">
          <Field label="分组">
            <input className={inputClassName} value={keyForm.groupName} onChange={(event) => onKeyFormChange({ ...keyForm, groupName: event.target.value })} />
          </Field>
          <Field label="档位">
            <input className={inputClassName} value={keyForm.tierLabel} onChange={(event) => onKeyFormChange({ ...keyForm, tierLabel: event.target.value })} />
          </Field>
          <Field label="状态">
            <SelectControl
              ariaLabel="密钥状态"
              className={inputClassName}
              value={keyForm.status}
              options={Object.entries(stationKeyStatusLabels).map(([value, label]) => ({
                value: value as StationKeyStatus,
                label,
              }))}
              onChange={(status) => onKeyFormChange({ ...keyForm, status })}
            />
          </Field>
        </div>
        <label className="flex items-center gap-2 text-sm text-foreground">
          <input checked={keyForm.enabled} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onKeyFormChange({ ...keyForm, enabled: event.target.checked })} />
          启用
        </label>
        <Field label="备注">
          <textarea className={`${inputClassName} min-h-20 resize-none py-2`} value={keyForm.note} onChange={(event) => onKeyFormChange({ ...keyForm, note: event.target.value })} />
        </Field>
      </form>
    </Dialog>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}
