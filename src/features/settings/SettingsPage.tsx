import { useState, type FormEvent, type ReactNode } from "react";
import { Coins, FolderOpen, Play, RotateCcw, Save, Square } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageActivation } from "@/components/shell/PageActivity";
import { Button, MaskedSecret, SectionCard, SelectControl, StatusBadge, SwitchControl, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import {
  getProxyStatus,
  restartLocalProxy,
  startLocalProxy,
  stopLocalProxy,
} from "@/lib/api/proxy";
import { chooseDataDir, getLocalAccessKey, getSettings, SETTINGS_UPDATED_EVENT, updateSettings } from "@/lib/api/settings";
import type { ProxyStatus } from "@/lib/types/proxy";
import {
  routingStrategyLabels,
  type AppSettings,
  type RoutingStrategy,
  type TrayBehavior,
  type UpdateSettingsInput,
} from "@/lib/types/settings";

type SettingsFormState = {
  localProxyPort: string;
  defaultRoutingStrategy: RoutingStrategy;
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

export function SettingsPage({ onOpenModelBasePrices }: SettingsPageProps) {
  const toast = useToast();
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus>(fallbackProxyStatus);
  const [form, setForm] = useState<SettingsFormState>(settingsToForm(fallbackSettings));
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [proxyBusy, setProxyBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  usePageActivation(({ isInitial }) => {
    void refreshSettings(isInitial);
  });

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

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await persistSettings(form, "设置已保存", false);
  }

  async function handleDeveloperModeToggle() {
    const nextForm = { ...form, developerModeEnabled: !form.developerModeEnabled };
    setForm(nextForm);
    await persistSettings(
      nextForm,
      nextForm.developerModeEnabled ? "开发者模式已开启" : "开发者模式已关闭",
      true,
    );
  }

  async function handleDefaultRoutingStrategyChange(defaultRoutingStrategy: RoutingStrategy) {
    const nextForm = { ...form, defaultRoutingStrategy };
    setForm(nextForm);
    await persistSettings(nextForm, "默认路由策略已更新", true);
  }

  async function copyLocalAccessKey() {
    const localAccessKey = await getLocalAccessKey();
    await navigator.clipboard.writeText(localAccessKey);
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

  async function persistSettings(
    nextForm: SettingsFormState,
    successMessage: string,
    revertOnFailure: boolean,
  ) {
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
      if (revertOnFailure) {
        setForm(settingsToForm(settings));
      }
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
      actions={
        <Button disabled={saving || loading} form="settings-form" type="submit">
          <Save className="h-4 w-4" />
          {saving ? "保存中" : "保存设置"}
        </Button>
      }
    >
      <form id="settings-form" className="grid min-w-0 gap-[var(--shell-page-gap)]" onSubmit={handleSubmit}>
        <SectionCard
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
            description={`活动请求 ${proxyStatus.activeRequests} 个；累计请求 ${proxyStatus.requestCount} 次；${proxyStatus.lastError ?? "最近没有运行时错误"}`}
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
              />
            }
            description="保存后重启代理生效；端口冲突会返回可读错误。"
            label="代理端口"
          />
          <SettingRow
            control={<code className="break-all text-xs text-slate-700">http://127.0.0.1:{settings.localProxyPort}/v1</code>}
            description="复制到 CCSwitch 或其他兼容客户端。"
            label="基础地址"
          />
          <SettingRow
            control={<MaskedSecret value={settings.localKeyMasked} onCopy={copyLocalAccessKey} />}
            description="只展示脱敏值；真实本地访问密钥由加密存储管理。"
            label="本地访问密钥"
          />
        </SectionCard>

        <SectionCard title="采集与路由">
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
              <input
                className={inputClassName}
                min="0"
                step="0.01"
                type="number"
                value={form.lowBalanceThresholdCny}
                onChange={(event) =>
                  setForm({ ...form, lowBalanceThresholdCny: event.target.value })
                }
              />
            }
            description="低于该值时，成本策略会降低或跳过低余额候选。"
            label="低余额阈值"
          />
          <SettingRow
            control={<SettingsNumberInput min="1" value={form.balanceIntervalMinutes} onChange={(balanceIntervalMinutes) => setForm({ ...form, balanceIntervalMinutes })} />}
            description="余额快照采集周期。"
            label="余额采集周期（分钟）"
          />
          <SettingRow
            control={<SettingsNumberInput min="1" value={form.groupRateIntervalMinutes} onChange={(groupRateIntervalMinutes) => setForm({ ...form, groupRateIntervalMinutes })} />}
            description="分组可见性和倍率事实采集周期。"
            label="分组 / 倍率采集周期（分钟）"
          />
          <SettingRow
            control={<SettingsNumberInput min="1" value={form.modelListIntervalMinutes} onChange={(modelListIntervalMinutes) => setForm({ ...form, modelListIntervalMinutes })} />}
            description="模型列表刷新周期。"
            label="模型采集周期（分钟）"
          />
          <SettingRow
            control={<SettingsNumberInput min="1" value={form.pricingRefreshIntervalMinutes} onChange={(pricingRefreshIntervalMinutes) => setForm({ ...form, pricingRefreshIntervalMinutes })} />}
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
            control={<SettingsNumberInput min="3" value={form.collectorTimeoutSeconds} onChange={(collectorTimeoutSeconds) => setForm({ ...form, collectorTimeoutSeconds })} />}
            description="单次采集请求超时；后端要求至少 3 秒。"
            label="采集超时（秒）"
          />
          <SettingRow
            control={<SettingsNumberInput max="8" min="1" value={form.collectorMaxConcurrency} onChange={(collectorMaxConcurrency) => setForm({ ...form, collectorMaxConcurrency })} />}
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
                onCheckedChange={() => setForm({ ...form, allowDepletedFallback: !form.allowDepletedFallback })}
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

        <SectionCard title="数据与安全">
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

        {error && <div className="text-sm text-rose-700">{error}</div>}
      </form>
    </PageScaffold>
  );
}

function SettingsNumberInput({
  value,
  min,
  max,
  onChange,
}: {
  value: string;
  min?: string;
  max?: string;
  onChange: (value: string) => void;
}) {
  return (
    <input
      className={inputClassName}
      max={max}
      min={min}
      type="number"
      value={value}
      onChange={(event) => onChange(event.target.value)}
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
    <div className="grid min-h-14 grid-cols-1 items-start gap-2 border-b border-border py-3 last:border-b-0 sm:grid-cols-[minmax(0,1fr)_minmax(180px,260px)] sm:items-center sm:gap-4">
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


const inputClassName =
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:bg-white focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]";
