import type { ReactNode } from "react";
import { Check, KeyRound, LogIn, Plus, RefreshCw, ShieldCheck } from "lucide-react";
import { Button, SectionCard, SelectControl } from "@/components/ui";
import { DEFAULT_MANUAL_PROXY_URL, withManualProxyDefault } from "@/lib/proxyDefaults";
import {
  stationProxyModeLabels,
  stationTypeOptions,
  type StationProxyMode,
  type StationType,
} from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import { providerPresets, type ProviderPresetId } from "../../providerPresets";
import { StationGroupRowsEditor, type StationGroupDraft } from "../../components/StationGroupRowsEditor";
import {
  StationKeyRowsEditor,
  type StationKeyDraft,
  type StationKeyGroupOption,
} from "../../components/StationKeyRowsEditor";
import { RemoteKeyDiscoveryList } from "../../components/RemoteKeyDiscoveryList";
import type { RemoteKeyCapability, RemoteStationKey, StationKey } from "@/lib/types/stationKeys";
import type { CommonLoginEmail, CommonLoginPassword } from "@/lib/types/settings";
import { inputClassName, type AddProviderFormState, type ConnectionTestState } from "./formModel";

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}

type ProviderPresetSectionProps = {
  presetId: ProviderPresetId;
  onApplyPreset: (presetId: ProviderPresetId) => void;
};

export function ProviderPresetSection({ presetId, onApplyPreset }: ProviderPresetSectionProps) {
  return (
    <SectionCard title="预设供应商">
      <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,9rem),1fr))] gap-2">
        {providerPresets.map((preset) => {
          const selected = preset.id === presetId;
          return (
            <button
              key={preset.id}
              type="button"
              className={cn(
                "relative flex h-8 min-w-0 cursor-pointer items-center gap-2 rounded-[var(--surface-radius)] px-2.5 text-left text-xs font-medium transition-colors",
                selected
                  ? "bg-primary-solid text-primary-foreground shadow-sm"
                  : "bg-muted text-muted-foreground hover:bg-hover hover:text-foreground",
              )}
              onClick={() => onApplyPreset(preset.id)}
              title={preset.description}
            >
              <span
                className={cn(
                  "flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-[5px] bg-surface text-[10px] font-semibold text-muted-foreground",
                  selected && "text-primary",
                )}
              >
                {preset.name.slice(0, 1)}
              </span>
              <span className="min-w-0 truncate">{preset.name}</span>
              {selected && <Check className="ml-auto h-3.5 w-3.5 shrink-0" />}
            </button>
          );
        })}
      </div>
    </SectionCard>
  );
}

type ProviderConnectionSectionProps = {
  commonLoginEmails: CommonLoginEmail[];
  commonLoginPasswords: CommonLoginPassword[];
  connectionTest: ConnectionTestState;
  editing: boolean;
  error: string | null;
  form: AddProviderFormState;
  loading: boolean;
  passwordProfileLoading: boolean;
  saving: boolean;
  startingAuthorization: boolean;
  testingConnection: boolean;
  onConnectionTestReset: () => void;
  onCommonEmailSelect: (profileId: string) => void;
  onCommonPasswordSelect: (profileId: string) => void;
  onCopyWebsiteUrl: () => void;
  onFormChange: (form: AddProviderFormState) => void;
  onStartManualAuthorization: () => void;
  onStationTypeChange: (stationType: StationType) => void;
  onTestConnection: () => void;
};

export function ProviderConnectionSection({
  commonLoginEmails,
  commonLoginPasswords,
  connectionTest,
  editing,
  error,
  form,
  loading,
  passwordProfileLoading,
  saving,
  startingAuthorization,
  testingConnection,
  onConnectionTestReset,
  onCommonEmailSelect,
  onCommonPasswordSelect,
  onCopyWebsiteUrl,
  onFormChange,
  onStartManualAuthorization,
  onStationTypeChange,
  onTestConnection,
}: ProviderConnectionSectionProps) {
  return (
    <SectionCard title="连接信息">
      <div className="grid gap-3 md:grid-cols-2">
        <Field label="供应商名称">
          <input
            className={inputClassName}
            value={form.name}
            onChange={(event) => onFormChange({ ...form, name: event.target.value })}
            placeholder="例如 我的供应商"
          />
        </Field>
        <Field label="站点类型">
          <SelectControl
            ariaLabel="站点类型"
            className={inputClassName}
            value={form.stationType}
            options={stationTypeOptions}
            onChange={onStationTypeChange}
          />
        </Field>
      </div>
      <div className="mt-3 grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-end">
        <Field label="前端网址">
          <input
            className={inputClassName}
            value={form.websiteUrl}
            onChange={(event) => {
              onFormChange({ ...form, websiteUrl: event.target.value });
              onConnectionTestReset();
            }}
            placeholder="https://example.com"
          />
        </Field>
        <Field label="API Base URL">
          <input
            className={inputClassName}
            value={form.apiBaseUrl}
            onChange={(event) => {
              onFormChange({ ...form, apiBaseUrl: event.target.value });
              onConnectionTestReset();
            }}
            placeholder="https://api.example.com/v1"
          />
        </Field>
        <Button
          variant="outline"
          className="whitespace-nowrap px-2.5"
          onClick={onCopyWebsiteUrl}
        >
          复制前端网址
        </Button>
      </div>
      <div className="mt-3 grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto_auto] md:items-end">
        <CompoundField label="登录用户名 / 邮箱">
          <FillableLoginInput
            ariaLabel="登录用户名或邮箱"
            disabled={loading}
            kind="email"
            options={commonLoginEmails.map((item) => ({
              value: item.id,
              label: item.email,
            }))}
            value={form.loginUsername}
            onChange={(value) => {
              onFormChange({ ...form, loginUsername: value });
              onConnectionTestReset();
            }}
            onProfileSelect={onCommonEmailSelect}
          />
        </CompoundField>
        <CompoundField label="登录密码">
          <FillableLoginInput
            ariaLabel="登录密码"
            disabled={loading || passwordProfileLoading}
            kind="password"
            options={commonLoginPasswords.map((item) => ({
              value: item.id,
              label: item.passwordMasked,
            }))}
            value={form.loginPassword}
            onChange={(value) => {
              onFormChange({
                ...form,
                loginPassword: value,
                rememberPassword: Boolean(value.trim()),
              });
              onConnectionTestReset();
            }}
            placeholder={editing ? "留空保留旧密码" : "用于采集登录"}
            onProfileSelect={onCommonPasswordSelect}
          />
        </CompoundField>
        <Button
          variant="outline"
          onClick={onStartManualAuthorization}
          disabled={saving || loading || startingAuthorization}
        >
          <LogIn className="h-4 w-4" />
          {startingAuthorization ? "打开中" : "打开窗口授权"}
        </Button>
        <Button
          variant="outline"
          onClick={onTestConnection}
          disabled={saving || testingConnection}
        >
          <ShieldCheck className="h-4 w-4" />
          {testingConnection ? "测试中" : "测试连通性"}
        </Button>
      </div>
      {connectionTest.message && (
        <div
          className={cn(
            "mt-2 min-w-0 truncate text-xs",
            connectionTest.status === "success" && "text-success-foreground",
            connectionTest.status === "warning" && "text-warning-foreground",
            connectionTest.status === "error" && "text-danger-foreground",
            connectionTest.status === "testing" && "text-muted-foreground",
          )}
        >
          {connectionTest.message}
        </div>
      )}
      {error && (
        <div className="mt-3 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
          {error}
        </div>
      )}
    </SectionCard>
  );
}

function CompoundField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      <div>{label}</div>
      {children}
    </div>
  );
}

function FillableLoginInput({
  ariaLabel,
  disabled,
  kind,
  options,
  value,
  placeholder,
  onChange,
  onProfileSelect,
}: {
  ariaLabel: string;
  disabled: boolean;
  kind: "email" | "password";
  options: Array<{ value: string; label: string }>;
  value: string;
  placeholder?: string;
  onChange: (value: string) => void;
  onProfileSelect: (profileId: string) => void;
}) {
  return (
    <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-2">
      <input
        aria-label={ariaLabel}
        autoComplete={kind === "email" ? "username" : "current-password"}
        className={cn(inputClassName, "min-w-0 w-full")}
        disabled={disabled}
        placeholder={placeholder ?? "user@example.com"}
        type={kind === "password" ? "password" : "text"}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
      <SelectControl
        ariaLabel={kind === "email" ? "选择常用邮箱" : "选择常用密码"}
        className="h-8 w-8 min-w-[2rem] justify-center gap-0 px-0 shadow-none"
        disabled={disabled || options.length === 0}
        menuAlign="end"
        menuMinWidth={220}
        options={options}
        placeholder={null}
        value=""
        onChange={onProfileSelect}
      />
    </div>
  );
}

type ProviderGroupsSectionProps = {
  developerModeEnabled: boolean;
  disabled: boolean;
  remoteCapabilityUnavailableReason: string | null;
  remoteLoading: boolean;
  rows: StationGroupDraft[];
  scanRemoteDisabled: boolean;
  onAddGroup: () => void;
  onRowsChange: (rows: StationGroupDraft[]) => void;
  onSyncRemoteGroups: () => void;
};

export function ProviderGroupsSection({
  developerModeEnabled,
  disabled,
  remoteCapabilityUnavailableReason,
  remoteLoading,
  rows,
  scanRemoteDisabled,
  onAddGroup,
  onRowsChange,
  onSyncRemoteGroups,
}: ProviderGroupsSectionProps) {
  return (
    <SectionCard
      title="分组"
      action={
        <div className="flex flex-wrap justify-end gap-2">
          <Button
            disabled={scanRemoteDisabled}
            size="sm"
            title={remoteCapabilityUnavailableReason ?? undefined}
            variant="outline"
            onClick={onSyncRemoteGroups}
          >
            <RefreshCw className={cn("h-3.5 w-3.5", remoteLoading && "animate-spin")} />
            同步远端分组
          </Button>
          <Button
            disabled={disabled}
            size="sm"
            variant="outline"
            onClick={onAddGroup}
          >
            <Plus className="h-3.5 w-3.5" />
            添加分组
          </Button>
        </div>
      }
    >
      <StationGroupRowsEditor
        developerModeEnabled={developerModeEnabled}
        disabled={disabled}
        rows={rows}
        onRowsChange={onRowsChange}
      />
    </SectionCard>
  );
}

type ProviderKeysSectionProps = {
  activeStationId: string | null;
  providerDraftId?: string | null;
  createRemoteDisabled: boolean;
  currentCreditPerCny: number;
  disabled: boolean;
  groupOptions: StationKeyGroupOption[];
  localKeyIdsCreatedByRemote: Record<string, string>;
  localKeys: StationKey[];
  remoteCapability: RemoteKeyCapability | null;
  remoteCapabilityError: string | null;
  remoteCapabilityUnavailableReason: string | null;
  remoteDiscoveryReason: string | null;
  remoteKeys: RemoteStationKey[];
  remoteListError: string | null;
  remoteLoading: boolean;
  remoteUnsupportedReason: string | null;
  rows: StationKeyDraft[];
  scanRemoteDisabled: boolean;
  onAddLocalKey: () => void;
  onBindRemoteKey: (remoteKeyId: string, stationKeyId: string) => void;
  onDeleteImportedLocalKey: (remoteKey: RemoteStationKey) => void;
  onDeleteRemoteKey: (remoteKey: RemoteStationKey) => void;
  onImportRemoteKey: (remoteKey: RemoteStationKey) => void;
  onOpenCreateRemoteKey: () => void;
  onRowsChange: (rows: StationKeyDraft[]) => void;
  onScanRemoteKeys: () => void;
  onUnbindRemoteKey: (remoteKey: RemoteStationKey) => void;
};

export function ProviderKeysSection({
  activeStationId,
  providerDraftId,
  createRemoteDisabled,
  currentCreditPerCny,
  disabled,
  groupOptions,
  localKeyIdsCreatedByRemote,
  localKeys,
  remoteCapability,
  remoteCapabilityError,
  remoteCapabilityUnavailableReason,
  remoteDiscoveryReason,
  remoteKeys,
  remoteListError,
  remoteLoading,
  remoteUnsupportedReason,
  rows,
  scanRemoteDisabled,
  onAddLocalKey,
  onBindRemoteKey,
  onDeleteImportedLocalKey,
  onDeleteRemoteKey,
  onImportRemoteKey,
  onOpenCreateRemoteKey,
  onRowsChange,
  onScanRemoteKeys,
  onUnbindRemoteKey,
}: ProviderKeysSectionProps) {
  return (
    <SectionCard
      title="密钥"
      action={
        <div className="flex flex-wrap justify-end gap-2">
          <Button
            disabled={scanRemoteDisabled}
            size="sm"
            title={remoteCapabilityUnavailableReason ?? undefined}
            variant="outline"
            onClick={onScanRemoteKeys}
          >
            <RefreshCw className={cn("h-3.5 w-3.5", remoteLoading && "animate-spin")} />
            获取所有 Key
          </Button>
          <Button
            disabled={createRemoteDisabled}
            size="sm"
            title={remoteCapabilityUnavailableReason ?? undefined}
            variant="secondary"
            onClick={onOpenCreateRemoteKey}
          >
            <Plus className="h-3.5 w-3.5" />
            新建远端 Key
          </Button>
          <Button
            disabled={disabled}
            size="sm"
            variant="outline"
            onClick={onAddLocalKey}
          >
            <Plus className="h-3.5 w-3.5" />
            添加密钥
          </Button>
        </div>
      }
    >
      <StationKeyRowsEditor
        disabled={disabled}
        groupOptions={groupOptions}
        rows={rows}
        onRowsChange={onRowsChange}
      />
      {(activeStationId || providerDraftId) && (
        <div className="mt-3 grid gap-2 border-t border-border pt-3">
          <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
            <KeyRound className="h-3.5 w-3.5" />
            远端发现
          </div>
          {remoteCapabilityError || (remoteUnsupportedReason && remoteCapability?.canListRemoteKeys !== true) ? (
            <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
              {remoteDiscoveryReason}
            </div>
          ) : (
            <>
              <RemoteKeyDiscoveryList
                creditPerCny={currentCreditPerCny}
                deleteDisabled={remoteCapability?.canDeleteRemoteKeys !== true}
                keys={remoteKeys}
                loading={remoteLoading}
                readOnly={!activeStationId}
                localKeyIdsCreatedByRemote={localKeyIdsCreatedByRemote}
                localKeys={localKeys}
                onBind={onBindRemoteKey}
                onDelete={onDeleteRemoteKey}
                onDeleteImportedLocalKey={onDeleteImportedLocalKey}
                onImport={onImportRemoteKey}
                onUnbind={onUnbindRemoteKey}
              />
              {remoteListError && (
                <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
                  {remoteDiscoveryReason}
                </div>
              )}
            </>
          )}
        </div>
      )}
    </SectionCard>
  );
}

type ProviderOptionsSectionProps = {
  form: AddProviderFormState;
  onFormChange: (form: AddProviderFormState) => void;
};

export function ProviderOptionsSection({ form, onFormChange }: ProviderOptionsSectionProps) {
  return (
    <SectionCard title="可选项">
      <div className="grid gap-3">
        <Field label="低余额阈值 CNY">
          <input
            className={inputClassName}
            min="0"
            step="0.01"
            type="number"
            value={form.lowBalanceThresholdCny}
            onChange={(event) => onFormChange({ ...form, lowBalanceThresholdCny: event.target.value })}
            placeholder="使用全局设置"
          />
        </Field>
        <Field label="兑换比例">
          <input
            className={inputClassName}
            min="0.01"
            step="0.01"
            type="number"
            value={form.creditPerCny}
            onChange={(event) => onFormChange({ ...form, creditPerCny: event.target.value })}
          />
        </Field>
        <Field label="采集频率 分钟">
          <input
            className={inputClassName}
            min="1"
            step="1"
            type="number"
            value={form.collectionIntervalMinutes}
            onChange={(event) => onFormChange({ ...form, collectionIntervalMinutes: event.target.value })}
            placeholder="5"
          />
        </Field>
        <Field label="采集代理">
          <div className="grid gap-2">
            <SelectControl
              ariaLabel="站点采集代理"
              className={inputClassName}
              value={form.collectorProxyMode}
              options={Object.entries(stationProxyModeLabels).map(([value, label]) => ({
                value: value as StationProxyMode,
                label,
              }))}
              onChange={(collectorProxyMode) => {
                const nextForm = { ...form, collectorProxyMode };
                onFormChange(
                  collectorProxyMode === "manual"
                    ? withManualProxyDefault(nextForm)
                    : nextForm,
                );
              }}
            />
            {form.collectorProxyMode === "manual" && (
              <input
                className={inputClassName}
                placeholder={DEFAULT_MANUAL_PROXY_URL}
                value={form.collectorProxyUrl}
                onChange={(event) => onFormChange({ ...form, collectorProxyUrl: event.target.value })}
              />
            )}
            <p className="text-xs text-muted-foreground">
              登录刷新、余额/分组采集、远端 Key 和本地 key 路由都会使用该站点的有效代理。
            </p>
          </div>
        </Field>
        <Field label="备注">
          <textarea
            className={`${inputClassName} min-h-24 resize-none py-2`}
            value={form.note}
            onChange={(event) => onFormChange({ ...form, note: event.target.value })}
            placeholder="登录方式、模型限制或计费说明"
          />
        </Field>
      </div>
    </SectionCard>
  );
}
