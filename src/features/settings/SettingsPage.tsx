import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { Play, RotateCcw, Save, Square } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, MaskedSecret, SectionCard, SelectControl, StatusBadge, useToast } from "@/components/ui";
import {
  getProxyStatus,
  restartLocalProxy,
  startLocalProxy,
  stopLocalProxy,
} from "@/lib/api/proxy";
import { getSecretMigrationStatus, runSecretSafetyScan } from "@/lib/api/secrets";
import { getSettings, updateSettings } from "@/lib/api/settings";
import type { ProxyStatus } from "@/lib/types/proxy";
import type { SecretMigrationReport, SecretScanFinding } from "@/lib/types/secrets";
import {
  routingStrategyLabels,
  trayBehaviorLabels,
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
  trayBehavior: TrayBehavior;
};

const fallbackSettings: AppSettings = {
  localProxyPort: 8787,
  localKeyMasked: "未读取",
  defaultRoutingStrategy: "priority_fallback",
  lowBalanceThresholdCny: 15,
  collectorIntervalMinutes: 30,
  trayBehavior: "minimize-to-tray",
  dataDir: "等待 Tauri 数据目录",
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

export function SettingsPage() {
  const toast = useToast();
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus>(fallbackProxyStatus);
  const [form, setForm] = useState<SettingsFormState>(settingsToForm(fallbackSettings));
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [proxyBusy, setProxyBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [secretMigration, setSecretMigration] = useState<SecretMigrationReport | null>(null);
  const [scanFindings, setScanFindings] = useState<SecretScanFinding[]>([]);
  const [securityError, setSecurityError] = useState<string | null>(null);

  useEffect(() => {
    void refreshSettings();
  }, []);

  async function refreshSettings() {
    setLoading(true);
    setError(null);
    try {
      const nextSettings = await getSettings();
      const nextProxyStatus = await getProxyStatus();
      await refreshSecurityStatus();
      setSettings(nextSettings);
      setProxyStatus(nextProxyStatus);
      setForm(settingsToForm(nextSettings));
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新设置失败", message);
    } finally {
      setLoading(false);
    }
  }

  async function refreshSecurityStatus() {
    setSecurityError(null);
    try {
      const [migration, findings] = await Promise.all([
        getSecretMigrationStatus(),
        runSecretSafetyScan(),
      ]);
      setSecretMigration(migration);
      setScanFindings(findings);
    } catch (requestError) {
      setSecurityError(readError(requestError));
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
    setSaving(true);
    setError(null);
    try {
      const nextSettings = await updateSettings(formToInput(form));
      setSettings(nextSettings);
      setForm(settingsToForm(nextSettings));
      toast.success("设置已保存");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("保存设置失败", message);
    } finally {
      setSaving(false);
    }
  }

  return (
    <PageScaffold
      title="设置"
      description="管理本地 OpenAI-compatible 代理、路由偏好和数据目录。"
      width="settings"
      actions={
        <Button disabled={saving || loading} form="settings-form" type="submit">
          <Save className="h-4 w-4" />
          {saving ? "保存中" : "保存设置"}
        </Button>
      }
    >
      <form id="settings-form" className="grid gap-[var(--shell-page-gap)]" onSubmit={handleSubmit}>
        <SectionCard
          title="本地代理"
          description="P5 MVP 仅监听 127.0.0.1，外部工具可使用这个 OpenAI-compatible 入口。"
          action={
            <StatusBadge tone={proxyStatus.running ? "healthy" : "disabled"}>
              {proxyStatus.running ? "运行中" : "已停止"}
            </StatusBadge>
          }
        >
          <SettingRow
            control={
              <div className="flex flex-wrap justify-end gap-2">
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
            control={<code className="text-xs text-slate-700">http://127.0.0.1:{settings.localProxyPort}/v1</code>}
            description="复制到 CCSwitch 或其他 OpenAI-compatible 客户端。"
            label="Base URL"
          />
          <SettingRow
            control={<MaskedSecret value={settings.localKeyMasked} />}
            description="P2.5 只脱敏展示；后续需要迁移到加密存储。"
            label="Local Key"
          />
        </SectionCard>

        <SectionCard title="路由与采集" description="P6 根据 Key 池 priority、能力范围和健康状态选择 Station Key。">
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
                onChange={(defaultRoutingStrategy) => setForm({ ...form, defaultRoutingStrategy })}
              />
            }
            description="价格与余额策略后续阶段接入；P6 先使用优先级、稳定性和备用模式。"
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
            description="低于该值时后续路由可过滤站点。"
            label="低余额阈值"
          />
          <SettingRow
            control={
              <input
                className={inputClassName}
                min="1"
                type="number"
                value={form.collectorIntervalMinutes}
                onChange={(event) =>
                  setForm({ ...form, collectorIntervalMinutes: event.target.value })
                }
              />
            }
            description="采集器接入后使用。"
            label="采集频率"
          />
          <SettingRow
            control={
              <SelectControl
                ariaLabel="托盘行为"
                className={inputClassName}
                value={form.trayBehavior}
                options={Object.entries(trayBehaviorLabels).map(([value, label]) => ({
                  value: value as TrayBehavior,
                  label,
                }))}
                onChange={(trayBehavior) => setForm({ ...form, trayBehavior })}
              />
            }
            description="系统托盘行为占位。"
            label="托盘行为"
          />
        </SectionCard>

        <SectionCard title="数据与安全">
          <SettingRow
            control={<code className="truncate text-xs text-slate-700">{settings.dataDir}</code>}
            description="SQLite 文件不在仓库目录。"
            label="数据目录"
          />
          <SettingRow
            control={<StatusBadge tone="healthy">已启用</StatusBadge>}
            description="Station Key、站点 API Key 和登录密码通过 SecretManager 写入本地加密存储。"
            label="加密存储"
          />
          <SettingRow
            control={
              <div className="text-right text-xs text-slate-700">
                {secretMigration
                  ? `已迁移 ${secretMigration.migratedCount} 项 · 失败 ${secretMigration.failedCount} 项`
                  : "等待检查"}
              </div>
            }
            description={
              secretMigration?.failures.length
                ? secretMigration.failures.slice(0, 2).join("；")
                : "旧明文凭据会在启动代理或写入凭据时迁移到 secrets 表。"
            }
            label="凭据迁移"
          />
          <SettingRow
            control={
              <StatusBadge tone={scanFindings.length > 0 ? "warning" : "healthy"}>
                {scanFindings.length > 0 ? `${scanFindings.length} 项` : "未发现"}
              </StatusBadge>
            }
            description={
              securityError
                ? `安全扫描暂不可用：${securityError}`
                : scanFindings.length > 0
                  ? scanFindings
                      .slice(0, 2)
                      .map((finding) => `${finding.tableName}.${finding.columnName}`)
                      .join("；")
                  : "未发现 P8 canary 明文残留。"
            }
            label="安全扫描"
          />
          <div className="rounded-[var(--surface-radius)] border border-cyan-200 bg-cyan-50/80 px-4 py-3 text-xs leading-5 text-cyan-900">
            默认列表 API 只返回脱敏值和 present 状态；request logs、route details 和 collector snapshots 在入库前统一脱敏。
          </div>
        </SectionCard>

        {error && <div className="text-sm text-rose-700">{error}</div>}
      </form>
    </PageScaffold>
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
    <div className="grid min-h-14 grid-cols-[minmax(0,1fr)_260px] items-center gap-4 border-b border-border py-3 last:border-b-0">
      <div className="min-w-0">
        <div className="text-sm font-medium text-slate-800">{label}</div>
        {description && (
          <div className="mt-0.5 text-xs text-muted-foreground">{description}</div>
        )}
      </div>
      <div className="min-w-0 justify-self-end">{control}</div>
    </div>
  );
}

function settingsToForm(settings: AppSettings): SettingsFormState {
  return {
    localProxyPort: String(settings.localProxyPort),
    defaultRoutingStrategy: settings.defaultRoutingStrategy,
    lowBalanceThresholdCny: String(settings.lowBalanceThresholdCny),
    collectorIntervalMinutes: String(settings.collectorIntervalMinutes),
    trayBehavior: settings.trayBehavior,
  };
}

function formToInput(form: SettingsFormState): UpdateSettingsInput {
  return {
    localProxyPort: Number(form.localProxyPort),
    defaultRoutingStrategy: form.defaultRoutingStrategy,
    lowBalanceThresholdCny: Number(form.lowBalanceThresholdCny),
    collectorIntervalMinutes: Number(form.collectorIntervalMinutes),
    trayBehavior: form.trayBehavior,
  };
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const inputClassName =
  "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:bg-white focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]";
