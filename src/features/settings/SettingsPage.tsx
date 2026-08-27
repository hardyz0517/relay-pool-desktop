import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ChevronRight, Copy, ExternalLink, FileText, FolderOpen, Github, Play, RefreshCw, RotateCcw, Square, Wand2 } from "lucide-react";
import relayPoolLogo from "@/assets/relay-pool-logo.png";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, SectionCard, SelectControl, StatusBadge, SwitchControl, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import { cn } from "@/lib/utils";
import {
  getProxyStatus,
  restartLocalProxy,
  startLocalProxy,
  stopLocalProxy,
} from "@/lib/api/proxy";
import { openExternalUrl } from "@/lib/api/external";
import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import { chooseDataDir, getLocalAccessKey, resetDataDir, updateLocalAccessKey, updateSettings } from "@/lib/api/settings";
import { queryKeys } from "@/lib/query/queryKeys";
import { proxyStatusQueryOptions, settingsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type { ProxyStatus } from "@/lib/types/proxy";
import { useUpdater } from "@/lib/updater/UpdaterProvider";
import { isUpdaterBusyPhase } from "@/lib/updater/updateState";
import { ThemeSettings } from "@/features/settings/ThemeSettings";
import { CommonLoginProfilesSettings } from "@/features/settings/CommonLoginProfilesSettings";
import { DataMigrationSection } from "@/features/settings/data-migration/DataMigrationSection";
import { DEFAULT_MANUAL_PROXY_URL, withManualProxyDefault } from "@/lib/proxyDefaults";
import {
  appSettingsToUpdateInput,
  collectorProxyModeLabels,
  DEFAULT_SCHEDULER_ADVANCED_SETTINGS,
  type AppSettings,
  type CollectorProxyMode,
  type UpdateSettingsInput,
} from "@/lib/types/settings";

type SettingsFormState = {
  localProxyPort: string;
  collectorProxyMode: CollectorProxyMode;
  collectorProxyUrl: string;
  developerModeEnabled: boolean;
  showDecisionExplanation: boolean;
};

const fallbackSettings: AppSettings = {
  localProxyPort: 8787,
  localProxyStartOnLaunch: false,
  localKeyMasked: "未读取",
  defaultRoutingStrategy: "automatic_balanced",
  collectorProxyMode: "direct",
  collectorProxyUrl: null,
  maxRateMultiplier: null,
  defaultRoutingGroupFilter: "all_groups",
  schedulerAdvancedSettings: DEFAULT_SCHEDULER_ADVANCED_SETTINGS,
  lowBalanceThresholdCny: 15,
  collectorIntervalMinutes: 30,
  balanceIntervalMinutes: 5,
  groupRateIntervalMinutes: 20,
  publishedStatusIntervalMinutes: 5,
  pricingRefreshIntervalMinutes: 60,
  collectorTimeoutSeconds: 30,
  collectorMaxConcurrency: 3,
  allowDepletedFallback: false,
  developerModeEnabled: false,
  showDecisionExplanation: false,
  dataDir: "仅桌面端可读取",
  pendingDataDir: null,
  dataDirChangeRequiresRestart: false,
};

const fallbackProxyStatus: ProxyStatus = {
  running: false,
  lifecycle: "stopped",
  bindAddr: "127.0.0.1",
  port: 8787,
  startedAt: null,
  lastError: null,
  activeRequests: 0,
  requestCount: 0,
};

const REPOSITORY_URL = "https://github.com/hardyz0517/relay-pool-desktop";

type SettingsPageProps = {
  onOpenModelBasePrices?: () => void;
};

export function SettingsPage({ onOpenModelBasePrices }: SettingsPageProps = {}) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const { state: updaterState, checkNow: checkForUpdates } = useUpdater();
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const proxyStatusQuery = useActivityQuery(proxyStatusQueryOptions());
  const settings = settingsQuery.data ?? fallbackSettings;
  const proxyStatus = proxyStatusQuery.data ?? fallbackProxyStatus;
  const [form, setForm] = useState<SettingsFormState>(settingsToForm(fallbackSettings));
  const [saving, setSaving] = useState(false);
  const [proxyBusy, setProxyBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [localAccessKeyEditing, setLocalAccessKeyEditing] = useState(false);
  const [localAccessKeySaving, setLocalAccessKeySaving] = useState(false);
  const [localAccessKeyDraft, setLocalAccessKeyDraft] = useState("");
  const [localAccessKeyOriginal, setLocalAccessKeyOriginal] = useState("");
  const localAccessKeyInputRef = useRef<HTMLInputElement | null>(null);
  const loading = settingsQuery.isLoading || proxyStatusQuery.isLoading;

  useEffect(() => {
    if (!settingsQuery.data) {
      return;
    }
    setForm(settingsToForm(settingsQuery.data));
  }, [settingsQuery.data]);

  useEffect(() => {
    const queryError = settingsQuery.error ?? proxyStatusQuery.error;
    if (!queryError) {
      return;
    }
    const message = readError(queryError);
    setError(message);
    toast.error("刷新设置失败", message);
  }, [proxyStatusQuery.error, settingsQuery.error, toast]);

  useEffect(() => {
    if (localAccessKeyEditing) {
      localAccessKeyInputRef.current?.focus();
      localAccessKeyInputRef.current?.select();
    }
  }, [localAccessKeyEditing]);

  async function handleProxyAction(action: "start" | "stop" | "restart") {
    setProxyBusy(true);
    setError(null);
    try {
      const nextStatus =
        action === "start"
          ? await startLocalProxy()
          : action === "stop"
            ? await stopLocalProxy()
            : await restartLocalProxy();
      queryClient.setQueryData(queryKeys.proxyStatus, nextStatus);
      toast.success(
        action === "stop"
          ? "本地代理已停止"
          : `本地代理运行于 http://${nextStatus.bindAddr}:${nextStatus.port}/v1。`,
      );
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("代理操作失败", message);
      try {
        queryClient.setQueryData(queryKeys.proxyStatus, await getProxyStatus());
      } catch {
        // Keep the visible error from the failed action.
      }
    } finally {
      setProxyBusy(false);
    }
  }

  async function handleDeveloperModeToggle() {
    const nextForm = { ...form, developerModeEnabled: !form.developerModeEnabled };
    await commitSettingsForm(
      nextForm,
      nextForm.developerModeEnabled ? "开发者模式已开启" : "开发者模式已关闭",
    );
  }

  async function handleCollectorProxyModeChange(collectorProxyMode: CollectorProxyMode) {
    const nextForm =
      collectorProxyMode === "manual"
        ? withManualProxyDefault({ ...form, collectorProxyMode })
        : { ...form, collectorProxyMode };
    await commitSettingsForm(nextForm, "默认网络出口已更新");
  }

  async function handleDecisionExplanationToggle() {
    const nextForm = { ...form, showDecisionExplanation: !form.showDecisionExplanation };
    await commitSettingsForm(nextForm, nextForm.showDecisionExplanation ? "路由决策解释已开启" : "路由决策解释已关闭");
  }

  async function openRuntimeLog(target: "file" | "directory") {
    try {
      const diagnostics = getActiveBackendClient().runtimeDiagnostics;
      if (!diagnostics) {
        throw new Error("运行日志仅支持桌面端");
      }
      if (target === "file") {
        await diagnostics.openRuntimeLogFile();
      } else {
        await diagnostics.openRuntimeLogDirectory();
      }
    } catch (requestError) {
      const message = readError(requestError);
      toast.error(target === "file" ? "打开运行日志失败" : "打开运行日志文件夹失败", message);
    }
  }

  async function copyLocalAccessKey(value?: string) {
    const localAccessKey = value ?? await getLocalAccessKey();
    await navigator.clipboard.writeText(localAccessKey);
  }

  async function beginLocalAccessKeyEdit(nextValue?: string) {
    setError(null);
    try {
      const localAccessKey = nextValue ?? await getLocalAccessKey();
      setLocalAccessKeyOriginal(nextValue ? "" : localAccessKey);
      setLocalAccessKeyDraft(localAccessKey);
      setLocalAccessKeyEditing(true);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("读取本地访问密钥失败", message);
    }
  }

  function handleGenerateLocalAccessKey() {
    void beginLocalAccessKeyEdit(generateLocalAccessKey());
  }

  async function handleLocalAccessKeyBlur() {
    if (!localAccessKeyEditing) {
      return;
    }
    const nextValue = localAccessKeyDraft.trim();
    setLocalAccessKeyEditing(false);
    if (!nextValue) {
      setLocalAccessKeyDraft("");
      toast.error("保存本地访问密钥失败", "本地访问密钥不能为空。");
      return;
    }
    if (nextValue === localAccessKeyOriginal) {
      return;
    }

    setLocalAccessKeySaving(true);
    setError(null);
    try {
      const nextSettings = await updateLocalAccessKey(nextValue);
      queryClient.setQueryData(queryKeys.settings, nextSettings);
      setForm(settingsToForm(nextSettings));
      toast.success("本地访问密钥已更新");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("保存本地访问密钥失败", message);
    } finally {
      setLocalAccessKeySaving(false);
      setLocalAccessKeyDraft("");
      setLocalAccessKeyOriginal("");
    }
  }

  function handleLocalAccessKeyKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      event.preventDefault();
      event.currentTarget.blur();
    }
    if (event.key === "Escape") {
      event.preventDefault();
      setLocalAccessKeyEditing(false);
      setLocalAccessKeyDraft("");
      setLocalAccessKeyOriginal("");
    }
  }

  async function handleChooseDataDir() {
    setSaving(true);
    setError(null);
    try {
      const nextSettings = await chooseDataDir();
      queryClient.setQueryData(queryKeys.settings, nextSettings);
      setForm(settingsToForm(nextSettings));
      if (nextSettings.dataDirChangeRequiresRestart) {
        toast.success("数据保存位置已更新", "重启后使用新的数据目录。");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("选择数据保存位置失败", message);
    } finally {
      setSaving(false);
    }
  }

  async function handleResetDataDir() {
    setSaving(true);
    setError(null);
    try {
      const nextSettings = await resetDataDir();
      queryClient.setQueryData(queryKeys.settings, nextSettings);
      setForm(settingsToForm(nextSettings));
      if (nextSettings.dataDirChangeRequiresRestart) {
        toast.success("数据保存位置已恢复默认", "重启后使用默认数据目录。");
      } else {
        toast.success("数据保存位置已是默认目录");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("恢复默认数据保存位置失败", message);
    } finally {
      setSaving(false);
    }
  }

  async function commitSettingsForm(nextForm: SettingsFormState, successMessage: string) {
    setForm(nextForm);
    await persistSettings(nextForm, successMessage);
  }

  async function persistSettings(nextForm: SettingsFormState, successMessage: string) {
    setSaving(true);
    setError(null);
    try {
      const nextSettings = await updateSettings(formToInput(nextForm, settings));
      queryClient.setQueryData(queryKeys.settings, nextSettings);
      setForm(settingsToForm(nextSettings));
      toast.success(successMessage);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      setForm(settingsToForm(settings));
      toast.error("保存设置失败", message);
    } finally {
      setSaving(false);
    }
  }

  const restartRequired = settings.dataDirChangeRequiresRestart;
  const displayedDataDir = settings.pendingDataDir ?? settings.dataDir;

  return (
    <PageScaffold title="设置" width="settings">
      <div className="grid min-w-0 gap-[var(--shell-page-gap)]">
        <ThemeSettings />
        <CommonLoginProfilesSettings />

        <SectionCard
          contentClassName="p-0"
          title="本地代理"
          action={
            <StatusBadge tone={proxyStatus.running ? "healthy" : "disabled"}>
              {proxyStatus.running ? "运行中" : "已停止"}
            </StatusBadge>
          }
        >
          <SettingRow
            control={
              <div className="flex w-full flex-wrap justify-start gap-2 sm:justify-end">
                <Button
                  disabled={proxyBusy || proxyStatus.running}
                  type="button"
                  variant="secondary"
                  onClick={() => void handleProxyAction("start")}
                >
                  <Play className="h-4 w-4" />
                  启动
                </Button>
                <Button
                  disabled={proxyBusy || !proxyStatus.running}
                  type="button"
                  variant="outline"
                  onClick={() => void handleProxyAction("stop")}
                >
                  <Square className="h-4 w-4" />
                  停止
                </Button>
                <Button
                  disabled={proxyBusy}
                  type="button"
                  variant="outline"
                  onClick={() => void handleProxyAction("restart")}
                >
                  <RotateCcw className="h-4 w-4" />
                  重启
                </Button>
              </div>
            }
            label="运行状态"
          />
          <SettingRow
            control={
              <input
                className={inputClassName}
                max="65535"
                min="1"
                type="number"
                value={form.localProxyPort}
                onChange={(event) => setForm({ ...form, localProxyPort: event.target.value })}
                onBlur={() => void commitSettingsForm(form, "代理端口已更新")}
              />
            }
            label="代理端口"
          />
          <SettingRow
            control={<code className="break-all text-xs text-muted-foreground">http://127.0.0.1:{settings.localProxyPort}/v1</code>}
            label="基础地址"
          />
          <SettingRow
            control={
              <LocalAccessKeyControl
                draft={localAccessKeyDraft}
                editing={localAccessKeyEditing}
                inputRef={localAccessKeyInputRef}
                saving={localAccessKeySaving}
                value={settings.localKeyMasked}
                onBlur={() => void handleLocalAccessKeyBlur()}
                onChange={setLocalAccessKeyDraft}
                onCopy={() => void copyLocalAccessKey(localAccessKeyEditing ? localAccessKeyDraft : undefined)}
                onEdit={() => void beginLocalAccessKeyEdit()}
                onGenerate={handleGenerateLocalAccessKey}
                onKeyDown={handleLocalAccessKeyKeyDown}
              />
            }
            label="本地访问密钥"
          />
        </SectionCard>

        <SectionCard contentClassName="p-0" title="网络与代理">
          <SettingRow
            control={
              <div className="grid w-full min-w-0 gap-2">
                <SelectControl
                  ariaLabel="默认网络出口"
                  className={inputClassName}
                  value={form.collectorProxyMode}
                  options={Object.entries(collectorProxyModeLabels).map(([value, label]) => ({
                    value: value as CollectorProxyMode,
                    label,
                  }))}
                  onChange={(collectorProxyMode) =>
                    void handleCollectorProxyModeChange(collectorProxyMode)
                  }
                />
                {form.collectorProxyMode === "manual" ? (
                  <input
                    className={inputClassName}
                    placeholder={DEFAULT_MANUAL_PROXY_URL}
                    value={form.collectorProxyUrl}
                    onChange={(event) =>
                      setForm({ ...form, collectorProxyUrl: event.target.value })
                    }
                    onBlur={() => void commitSettingsForm(form, "默认网络出口已更新")}
                  />
                ) : null}
              </div>
            }
            description="采集与转发默认使用；站点可单独覆盖。"
            label="默认网络出口"
          />
        </SectionCard>

        {onOpenModelBasePrices ? (
          <SectionCard contentClassName="p-0" title="模型定价">
            <SettingRow
              control={
                <Button type="button" variant="outline" onClick={onOpenModelBasePrices}>
                  管理
                  <ChevronRight className="h-4 w-4" />
                </Button>
              }
              description="管理成本计算和价格比较使用的模型基准价格。"
              label="模型基准价格"
            />
          </SectionCard>
        ) : null}

        <SectionCard contentClassName="p-0" title="数据与备份">
          <SettingRow
            control={
              <div className="flex w-full min-w-0 items-center gap-1.5">
                <div
                  aria-label="当前数据目录"
                  className="data-dir-path-field flex h-8 min-w-0 flex-1 items-center rounded-[var(--surface-radius)] border border-border bg-control px-3 text-left font-mono text-xs text-foreground"
                  title={displayedDataDir}
                >
                  <span className="block truncate">{displayedDataDir}</span>
                </div>
                <Button
                  aria-label="选择数据目录位置"
                  className="shrink-0"
                  disabled={saving || loading}
                  size="icon"
                  title="选择位置"
                  type="button"
                  variant="outline"
                  onClick={() => void handleChooseDataDir()}
                >
                  <FolderOpen className="h-4 w-4" />
                </Button>
                <Button
                  aria-label="恢复默认数据目录"
                  className="shrink-0"
                  disabled={saving || loading}
                  size="icon"
                  title="恢复默认位置"
                  type="button"
                  variant="outline"
                  onClick={() => void handleResetDataDir()}
                >
                  <RotateCcw className="h-4 w-4" />
                </Button>
              </div>
            }
            description={restartRequired ? "重启后使用新的数据目录；当前运行仍使用原数据库。" : undefined}
            label="数据目录"
          />
          <DataMigrationSection />
        </SectionCard>

        <SectionCard contentClassName="p-0" title="高级">
          <SettingRow
            control={
              <SwitchControl
                ariaLabel="开发者模式"
                checked={form.developerModeEnabled}
                disabled={saving || loading}
                onCheckedChange={() => void handleDeveloperModeToggle()}
                showLabel={false}
              />
            }
            label="开发者模式"
          />
          <SettingRow
            control={
              <SwitchControl
                ariaLabel="显示路由决策解释"
                checked={form.showDecisionExplanation}
                disabled={saving || loading}
                onCheckedChange={() => void handleDecisionExplanationToggle()}
                showLabel={false}
              />
            }
            description="在请求详情中显示重试、等待、切换或停止的原因；不会改变路由行为。"
            label="显示路由决策解释"
          />
          <SettingRow
            control={
              <div className="flex flex-wrap items-center justify-end gap-2">
                <Button type="button" variant="outline" onClick={() => void openRuntimeLog("file")}>
                  <FileText className="h-4 w-4" />
                  打开日志
                </Button>
                <Button type="button" variant="outline" onClick={() => void openRuntimeLog("directory")}>
                  <FolderOpen className="h-4 w-4" />
                  打开日志文件夹
                </Button>
              </div>
            }
            label="运行日志"
          />
        </SectionCard>

        <SectionCard title="关于">
          <UpdateSettingsCard
            state={updaterState}
            onCheckForUpdates={() => void checkForUpdates()}
            onOpenGitHub={() => void openExternalUrl(REPOSITORY_URL)}
            onOpenReleaseLog={() => void openExternalUrl(currentReleaseUrl(updaterState.currentVersion))}
          />
        </SectionCard>

        {error && <div className="text-sm text-danger-foreground">{error}</div>}
      </div>
    </PageScaffold>
  );
}

function UpdateSettingsCard({
  state,
  onCheckForUpdates,
  onOpenGitHub,
  onOpenReleaseLog,
}: {
  state: ReturnType<typeof useUpdater>["state"];
  onCheckForUpdates: () => void;
  onOpenGitHub: () => void;
  onOpenReleaseLog: () => void;
}) {
  return (
    <div className="flex min-h-[88px] flex-col gap-4 px-5 py-4 sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <img
            src={relayPoolLogo}
            alt="Relay Pool Desktop"
            className="h-8 w-8 shrink-0 object-contain"
            draggable={false}
          />
          <div className="truncate text-base font-semibold text-foreground">Relay Pool</div>
        </div>
        <div className="mt-2 inline-flex h-6 items-center rounded-full border border-border bg-muted px-2 text-xs font-medium text-muted-foreground">
          版本 v{state.currentVersion}
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2 sm:justify-end">
        <Button type="button" variant="outline" onClick={onOpenGitHub}>
          <Github className="h-4 w-4" />
          GitHub
        </Button>
        <Button type="button" variant="outline" onClick={onOpenReleaseLog}>
          <ExternalLink className="h-4 w-4" />
          更新日志
        </Button>
        <Button disabled={isUpdaterBusyPhase(state.phase)} type="button" onClick={onCheckForUpdates}>
          <RefreshCw className={state.phase === "checking" ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
          检查更新
        </Button>
      </div>
    </div>
  );
}

function LocalAccessKeyControl({
  value,
  draft,
  editing,
  saving,
  inputRef,
  onEdit,
  onGenerate,
  onCopy,
  onChange,
  onBlur,
  onKeyDown,
}: {
  value: string;
  draft: string;
  editing: boolean;
  saving: boolean;
  inputRef: RefObject<HTMLInputElement>;
  onEdit: () => void;
  onGenerate: () => void;
  onCopy: () => void;
  onChange: (value: string) => void;
  onBlur: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void;
}) {
  const fieldClassName =
    "local-access-key-field h-8 w-[176px] flex-none rounded-[var(--surface-radius)] border border-border bg-control px-2 font-mono text-xs text-foreground transition";
  return (
    <div className="flex w-full min-w-0 items-center justify-start gap-1.5 sm:justify-end">
      {editing ? (
        <input
          ref={inputRef}
          aria-label="本地访问密钥"
          className={`${fieldClassName} border-ring bg-control text-foreground outline-none ring-2 ring-ring/20`}
          value={draft}
          onBlur={onBlur}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={onKeyDown}
        />
      ) : (
        <button
          className={`${fieldClassName} cursor-text text-left hover:border-ring hover:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30`}
          disabled={saving}
          type="button"
          onClick={onEdit}
        >
          <code className="block truncate">{saving ? "保存中" : value}</code>
        </button>
      )}
      <Button
        className="h-6 px-1.5 text-xs"
        disabled={saving}
        type="button"
        variant="ghost"
        onClick={onCopy}
        onMouseDown={(event) => event.preventDefault()}
      >
        <Copy className="h-3.5 w-3.5" />
        <span className="sr-only">复制</span>
      </Button>
      <Button
        className="h-6 w-6 px-0"
        disabled={saving}
        type="button"
        variant="ghost"
        onClick={onGenerate}
        onMouseDown={(event) => event.preventDefault()}
      >
        <Wand2 className="h-3.5 w-3.5" />
        <span className="sr-only">随机生成</span>
      </Button>
    </div>
  );
}

function SettingRow({
  label,
  description,
  control,
}: {
  label: string;
  description?: string;
  control: ReactNode;
}) {
  return (
    <div
      className={cn(
        "grid grid-cols-1 items-start gap-2 border-b border-border last:border-b-0 sm:grid-cols-[minmax(0,1fr)_minmax(180px,260px)] sm:items-center sm:gap-4",
        description ? "min-h-14 px-3 py-3" : "min-h-12 px-3 py-0",
      )}
    >
      <div className="min-w-0">
        <div className="text-sm font-medium text-foreground">{label}</div>
        {description && (
          <div className="mt-0.5 break-words text-xs text-muted-foreground">{description}</div>
        )}
      </div>
      <div className="min-w-0 w-full justify-self-stretch sm:w-auto sm:justify-self-end">{control}</div>
    </div>
  );
}

function settingsToForm(settings: AppSettings): SettingsFormState {
  return {
    localProxyPort: String(settings.localProxyPort),
    collectorProxyMode: settings.collectorProxyMode,
    collectorProxyUrl: settings.collectorProxyUrl ?? "",
    developerModeEnabled: settings.developerModeEnabled,
    showDecisionExplanation: settings.showDecisionExplanation ?? false,
  };
}

function formToInput(form: SettingsFormState, settings: AppSettings): UpdateSettingsInput {
  return {
    ...appSettingsToUpdateInput(settings),
    localProxyPort: Number(form.localProxyPort),
    defaultRoutingStrategy: "automatic_balanced",
    collectorProxyMode: form.collectorProxyMode,
    collectorProxyUrl:
      form.collectorProxyMode === "manual" && form.collectorProxyUrl.trim()
        ? form.collectorProxyUrl.trim()
        : null,
    schedulerAdvancedSettings: settings.schedulerAdvancedSettings,
    developerModeEnabled: form.developerModeEnabled,
    showDecisionExplanation: form.showDecisionExplanation,
  };
}

function generateLocalAccessKey() {
  const bytes = new Uint8Array(24);
  if (globalThis.crypto?.getRandomValues) {
    globalThis.crypto.getRandomValues(bytes);
  } else {
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = Math.floor(Math.random() * 256);
    }
  }
  const token = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `sk-local-${token}`;
}

const inputClassName =
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-control px-3 text-sm text-foreground outline-none transition focus:border-ring focus:bg-control focus:ring-2 focus:ring-ring/30";

function currentReleaseUrl(currentVersion: string) {
  const normalizedVersion = currentVersion.trim().replace(/^v/i, "");
  return `${REPOSITORY_URL}/releases/tag/v${normalizedVersion || "0.0.0"}`;
}
