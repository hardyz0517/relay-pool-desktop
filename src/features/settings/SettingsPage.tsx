import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { Coins, Copy, ExternalLink, FolderOpen, Github, Play, RefreshCw, RotateCcw, Square, Wand2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageActivation } from "@/components/shell/PageActivity";
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
import { chooseDataDir, getLocalAccessKey, getSettings, SETTINGS_UPDATED_EVENT, updateLocalAccessKey, updateSettings } from "@/lib/api/settings";
import type { ProxyStatus } from "@/lib/types/proxy";
import { useUpdater } from "@/features/updater/UpdaterProvider";
import { DEFAULT_MANUAL_PROXY_URL, withManualProxyDefault } from "@/lib/proxyDefaults";
import {
  collectorProxyModeLabels,
  routingStrategyLabels,
  type AppSettings,
  type CollectorProxyMode,
  type RoutingStrategy,
  type TrayBehavior,
  type UpdateSettingsInput,
} from "@/lib/types/settings";

type SettingsFormState = {
  localProxyPort: string;
  defaultRoutingStrategy: RoutingStrategy;
  collectorProxyMode: CollectorProxyMode;
  collectorProxyUrl: string;
  lowBalanceThresholdCny: string;
  collectorIntervalMinutes: string;
  balanceIntervalMinutes: string;
  groupRateIntervalMinutes: string;
  modelListIntervalMinutes: string;
  pricingRefreshIntervalMinutes: string;
  collectorTimeoutSeconds: string;
  collectorMaxConcurrency: string;
  allowDepletedFallback: boolean;
  trayBehavior: TrayBehavior;
  developerModeEnabled: boolean;
};

const fallbackSettings: AppSettings = {
  localProxyPort: 8787,
  localKeyMasked: "未读取",
  defaultRoutingStrategy: "cost_stable_first",
  collectorProxyMode: "direct",
  collectorProxyUrl: null,
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

const fallbackProxyStatus: ProxyStatus = {
  running: false,
  bindAddr: "127.0.0.1",
  port: 8787,
  startedAt: null,
  lastError: null,
  activeRequests: 0,
  requestCount: 0,
};

type SettingsPageProps = {
  onOpenModelBasePrices: () => void;
};

const REPOSITORY_URL = "https://github.com/hardyz0517/relay-pool-desktop";

export function SettingsPage({ onOpenModelBasePrices }: SettingsPageProps) {
  const toast = useToast();
  const { state: updaterState, checkNow: checkForUpdates } = useUpdater();
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus>(fallbackProxyStatus);
  const [form, setForm] = useState<SettingsFormState>(settingsToForm(fallbackSettings));
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [proxyBusy, setProxyBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [localAccessKeyEditing, setLocalAccessKeyEditing] = useState(false);
  const [localAccessKeySaving, setLocalAccessKeySaving] = useState(false);
  const [localAccessKeyDraft, setLocalAccessKeyDraft] = useState("");
  const [localAccessKeyOriginal, setLocalAccessKeyOriginal] = useState("");
  const localAccessKeyInputRef = useRef<HTMLInputElement | null>(null);

  usePageActivation(({ isInitial }) => {
    void refreshSettings(isInitial);
  });

  useEffect(() => {
    if (localAccessKeyEditing) {
      localAccessKeyInputRef.current?.focus();
      localAccessKeyInputRef.current?.select();
    }
  }, [localAccessKeyEditing]);

  async function refreshSettings(showLoading = true) {
    if (showLoading) {
      setLoading(true);
    }
    setError(null);
    try {
      const nextSettings = await getSettings();
      const nextProxyStatus = await getProxyStatus();
      setSettings(nextSettings);
      setProxyStatus(nextProxyStatus);
      setForm(settingsToForm(nextSettings));
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新设置失败", message);
    } finally {
      if (showLoading) {
        setLoading(false);
      }
    }
  }

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
      setProxyStatus(nextStatus);
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
        setProxyStatus(await getProxyStatus());
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

  async function handleDefaultRoutingStrategyChange(defaultRoutingStrategy: RoutingStrategy) {
    const nextForm = { ...form, defaultRoutingStrategy };
    await commitSettingsForm(nextForm, "默认路由策略已更新");
  }

  async function handleCollectorProxyModeChange(collectorProxyMode: CollectorProxyMode) {
    const nextForm =
      collectorProxyMode === "manual"
        ? withManualProxyDefault({ ...form, collectorProxyMode })
        : { ...form, collectorProxyMode };
    await commitSettingsForm(nextForm, "默认采集代理已更新");
  }

  async function handleAllowDepletedFallbackToggle() {
    const nextForm = { ...form, allowDepletedFallback: !form.allowDepletedFallback };
    await commitSettingsForm(
      nextForm,
      nextForm.allowDepletedFallback ? "余额耗尽兜底已开启" : "余额耗尽兜底已关闭",
    );
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
      setSettings(nextSettings);
      setForm(settingsToForm(nextSettings));
      window.dispatchEvent(new CustomEvent<AppSettings>(SETTINGS_UPDATED_EVENT, { detail: nextSettings }));
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
      setSettings(nextSettings);
      setForm(settingsToForm(nextSettings));
      window.dispatchEvent(new CustomEvent<AppSettings>(SETTINGS_UPDATED_EVENT, { detail: nextSettings }));
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

  async function commitSettingsForm(nextForm: SettingsFormState, successMessage: string) {
    setForm(nextForm);
    await persistSettings(nextForm, successMessage);
  }

  async function persistSettings(nextForm: SettingsFormState, successMessage: string) {
    setSaving(true);
    setError(null);
    try {
      const nextSettings = await updateSettings(formToInput(nextForm));
      setSettings(nextSettings);
      setForm(settingsToForm(nextSettings));
      window.dispatchEvent(new CustomEvent<AppSettings>(SETTINGS_UPDATED_EVENT, { detail: nextSettings }));
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

  return (
    <PageScaffold
      title="设置"
      width="settings"
    >
      <div className="grid min-w-0 gap-[var(--shell-page-gap)]">
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
            control={<code className="break-all text-xs text-slate-700">http://127.0.0.1:{settings.localProxyPort}/v1</code>}
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

        <SectionCard contentClassName="p-0" title="采集与路由">
          <SettingRow
            control={
              <SelectControl
                ariaLabel="默认路由策略"
                className={inputClassName}
                value={form.defaultRoutingStrategy}
                options={Object.entries(routingStrategyLabels).map(([value, label]) => ({
                  value: value as RoutingStrategy,
                  label,
                }))}
                onChange={(defaultRoutingStrategy) => void handleDefaultRoutingStrategyChange(defaultRoutingStrategy)}
              />
            }
            description="当前本地代理会按所选策略对密钥池候选排序。"
            label="默认路由策略"
          />
          <SettingRow
            control={
              <div className="grid w-full min-w-0 gap-2">
                <SelectControl
                  ariaLabel="默认采集代理"
                  className={inputClassName}
                  value={form.collectorProxyMode}
                  options={Object.entries(collectorProxyModeLabels).map(([value, label]) => ({
                    value: value as CollectorProxyMode,
                    label,
                  }))}
                  onChange={(collectorProxyMode) => void handleCollectorProxyModeChange(collectorProxyMode)}
                />
                {form.collectorProxyMode === "manual" && (
                  <input
                    className={inputClassName}
                    placeholder={DEFAULT_MANUAL_PROXY_URL}
                    value={form.collectorProxyUrl}
                    onChange={(event) =>
                      setForm({ ...form, collectorProxyUrl: event.target.value })
                    }
                    onBlur={() => void commitSettingsForm(form, "默认采集代理已更新")}
                  />
                )}
              </div>
            }
            description="登录刷新、余额/分组采集、远端 Key 读取和本地 key 路由都会使用这里的默认出口；单个中转站可以覆盖。"
            label="默认采集代理"
          />
          <SettingRow
            control={
              <input
                className={inputClassName}
                min="0"
                step="0.01"
                type="number"
                value={form.lowBalanceThresholdCny}
                onChange={(event) =>
                  setForm({ ...form, lowBalanceThresholdCny: event.target.value })
                }
                onBlur={() => void commitSettingsForm(form, "低余额阈值已更新")}
              />
            }
            description="低于该值时，成本策略会降低或跳过低余额候选。"
            label="低余额阈值"
          />
          <SettingRow
            control={<SettingsNumberInput min="1" value={form.balanceIntervalMinutes} onChange={(balanceIntervalMinutes) => setForm({ ...form, balanceIntervalMinutes })} onCommit={() => void commitSettingsForm(form, "余额采集周期已更新")} />}
            description="余额快照采集周期。"
            label="余额采集周期（分钟）"
          />
          <SettingRow
            control={<SettingsNumberInput min="1" value={form.groupRateIntervalMinutes} onChange={(groupRateIntervalMinutes) => setForm({ ...form, groupRateIntervalMinutes })} onCommit={() => void commitSettingsForm(form, "分组 / 倍率采集周期已更新")} />}
            description="分组可见性和倍率事实采集周期。"
            label="分组 / 倍率采集周期（分钟）"
          />
          <SettingRow
            control={<SettingsNumberInput min="1" value={form.modelListIntervalMinutes} onChange={(modelListIntervalMinutes) => setForm({ ...form, modelListIntervalMinutes })} onCommit={() => void commitSettingsForm(form, "模型采集周期已更新")} />}
            description="模型列表刷新周期。"
            label="模型采集周期（分钟）"
          />
          <SettingRow
            control={<SettingsNumberInput min="1" value={form.pricingRefreshIntervalMinutes} onChange={(pricingRefreshIntervalMinutes) => setForm({ ...form, pricingRefreshIntervalMinutes })} onCommit={() => void commitSettingsForm(form, "价格刷新周期已更新")} />}
            description="价格规则和倍率归一化刷新周期。"
            label="价格刷新周期（分钟）"
          />
          <SettingRow
            control={
              <Button type="button" variant="outline" onClick={onOpenModelBasePrices}>
                <Coins className="h-4 w-4" />
                编辑
              </Button>
            }
            description="用于把站点分组倍率折算成请求成本；默认值来自官方 API 定价页，可手动覆盖。"
            label="模型基准价格"
          />
          <SettingRow
            control={<SettingsNumberInput min="3" value={form.collectorTimeoutSeconds} onChange={(collectorTimeoutSeconds) => setForm({ ...form, collectorTimeoutSeconds })} onCommit={() => void commitSettingsForm(form, "采集超时已更新")} />}
            description="单次采集请求超时；后端要求至少 3 秒。"
            label="采集超时（秒）"
          />
          <SettingRow
            control={<SettingsNumberInput max="8" min="1" value={form.collectorMaxConcurrency} onChange={(collectorMaxConcurrency) => setForm({ ...form, collectorMaxConcurrency })} onCommit={() => void commitSettingsForm(form, "采集并发数已更新")} />}
            description="采集任务最大并发数；后端限制 1 到 8。"
            label="采集并发数"
          />
          <SettingRow
            control={
              <SwitchControl
                ariaLabel="允许余额耗尽兜底"
                checked={form.allowDepletedFallback}
                offLabel="关闭"
                onLabel="开启"
                onCheckedChange={() => void handleAllowDepletedFallbackToggle()}
              />
            }
            description="关闭时，余额耗尽的候选默认不参与路由；开启后只作为兜底候选。"
            label="允许余额耗尽兜底"
          />
          <SettingRow
            control={
              <SwitchControl
                ariaLabel="开发者模式"
                checked={form.developerModeEnabled}
                disabled={saving || loading}
                offLabel="关闭"
                onLabel="开启"
                onCheckedChange={() => void handleDeveloperModeToggle()}
              />
            }
            description="打开后侧边栏显示采集中心。"
            label="开发者模式"
          />
        </SectionCard>

        <SectionCard contentClassName="p-0" title="数据与安全">
          <SettingRow
            control={
              <div className="flex min-w-0 flex-col items-start gap-2 sm:items-end">
                <code className="break-all text-xs text-slate-700">
                  {settings.pendingDataDir ?? settings.dataDir}
                </code>
                <Button
                  disabled={saving || loading}
                  type="button"
                  variant="outline"
                  onClick={() => void handleChooseDataDir()}
                >
                  <FolderOpen className="h-4 w-4" />
                  选择位置
                </Button>
              </div>
            }
            description={
              restartRequired
                ? "重启后使用新的数据目录；当前运行仍使用原数据库。"
                : "本地数据库不在仓库目录。"
            }
            label="数据目录"
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

        {error && <div className="text-sm text-rose-700">{error}</div>}
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
          <span className="flex h-6 w-6 items-center justify-center rounded-full bg-cyan-50 text-sm text-cyan-700">
            ✺
          </span>
          <div className="truncate text-base font-semibold text-slate-950">Relay Pool</div>
        </div>
        <div className="mt-2 inline-flex h-6 items-center rounded-full border border-border bg-white px-2 text-xs font-medium text-slate-700">
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
        <Button disabled={state.phase === "checking"} type="button" onClick={onCheckForUpdates}>
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
    "local-access-key-field h-8 w-[176px] flex-none rounded-[var(--surface-radius)] border border-border bg-slate-50 px-2 font-mono text-xs text-slate-700 transition";
  return (
    <div className="flex w-full min-w-0 items-center justify-start gap-1.5 sm:justify-end">
      {editing ? (
        <input
          ref={inputRef}
          aria-label="本地访问密钥"
          className={`${fieldClassName} border-[hsl(var(--accent)/0.45)] bg-white text-slate-800 outline-none ring-2 ring-[hsl(var(--accent)/0.16)]`}
          value={draft}
          onBlur={onBlur}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={onKeyDown}
        />
      ) : (
        <button
          className={`${fieldClassName} cursor-text text-left hover:border-[hsl(var(--accent)/0.45)] hover:bg-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.2)]`}
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

function SettingsNumberInput({
  value,
  min,
  max,
  onChange,
  onCommit,
}: {
  value: string;
  min?: string;
  max?: string;
  onChange: (value: string) => void;
  onCommit: () => void;
}) {
  return (
    <input
      className={inputClassName}
      max={max}
      min={min}
      type="number"
      value={value}
      onChange={(event) => onChange(event.target.value)}
      onBlur={onCommit}
    />
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
        <div className="text-sm font-medium text-slate-800">{label}</div>
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
    defaultRoutingStrategy: settings.defaultRoutingStrategy,
    collectorProxyMode: settings.collectorProxyMode,
    collectorProxyUrl: settings.collectorProxyUrl ?? "",
    lowBalanceThresholdCny: String(settings.lowBalanceThresholdCny),
    collectorIntervalMinutes: String(settings.collectorIntervalMinutes),
    balanceIntervalMinutes: String(settings.balanceIntervalMinutes),
    groupRateIntervalMinutes: String(settings.groupRateIntervalMinutes),
    modelListIntervalMinutes: String(settings.modelListIntervalMinutes),
    pricingRefreshIntervalMinutes: String(settings.pricingRefreshIntervalMinutes),
    collectorTimeoutSeconds: String(settings.collectorTimeoutSeconds),
    collectorMaxConcurrency: String(settings.collectorMaxConcurrency),
    allowDepletedFallback: settings.allowDepletedFallback,
    trayBehavior: settings.trayBehavior,
    developerModeEnabled: settings.developerModeEnabled,
  };
}

function formToInput(form: SettingsFormState): UpdateSettingsInput {
  return {
    localProxyPort: Number(form.localProxyPort),
    defaultRoutingStrategy: form.defaultRoutingStrategy,
    collectorProxyMode: form.collectorProxyMode,
    collectorProxyUrl: form.collectorProxyMode === "manual" && form.collectorProxyUrl.trim()
      ? form.collectorProxyUrl.trim()
      : null,
    lowBalanceThresholdCny: Number(form.lowBalanceThresholdCny),
    collectorIntervalMinutes: Number(form.collectorIntervalMinutes),
    balanceIntervalMinutes: Number(form.balanceIntervalMinutes),
    groupRateIntervalMinutes: Number(form.groupRateIntervalMinutes),
    modelListIntervalMinutes: Number(form.modelListIntervalMinutes),
    pricingRefreshIntervalMinutes: Number(form.pricingRefreshIntervalMinutes),
    collectorTimeoutSeconds: Number(form.collectorTimeoutSeconds),
    collectorMaxConcurrency: Number(form.collectorMaxConcurrency),
    allowDepletedFallback: form.allowDepletedFallback,
    trayBehavior: form.trayBehavior,
    developerModeEnabled: form.developerModeEnabled,
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
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:bg-white focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]";

function currentReleaseUrl(currentVersion: string) {
  const normalizedVersion = currentVersion.trim().replace(/^v/i, "");
  return `${REPOSITORY_URL}/releases/tag/v${normalizedVersion || "0.0.0"}`;
}
