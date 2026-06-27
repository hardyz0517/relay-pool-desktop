import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { Play, RotateCcw, Save, Square } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, MaskedSecret, SectionCard, StatusBadge } from "@/components/ui";
import {
  getProxyStatus,
  restartLocalProxy,
  startLocalProxy,
  stopLocalProxy,
} from "@/lib/api/proxy";
import { getSettings, updateSettings } from "@/lib/api/settings";
import type { ProxyStatus } from "@/lib/types/proxy";
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
  defaultRoutingStrategy: "manual",
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
};

export function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus>(fallbackProxyStatus);
  const [form, setForm] = useState<SettingsFormState>(settingsToForm(fallbackSettings));
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [proxyBusy, setProxyBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refreshSettings();
  }, []);

  async function refreshSettings() {
    setLoading(true);
    setError(null);
    try {
      const nextSettings = await getSettings();
      const nextProxyStatus = await getProxyStatus();
      setSettings(nextSettings);
      setProxyStatus(nextProxyStatus);
      setForm(settingsToForm(nextSettings));
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setLoading(false);
    }
  }

  async function handleProxyAction(action: "start" | "stop" | "restart") {
    setProxyBusy(true);
    setMessage(null);
    setError(null);
    try {
      const nextStatus =
        action === "start"
          ? await startLocalProxy()
          : action === "stop"
            ? await stopLocalProxy()
            : await restartLocalProxy();
      setProxyStatus(nextStatus);
      setMessage(
        action === "stop"
          ? "本地代理已停止。"
          : `本地代理运行于 http://${nextStatus.bindAddr}:${nextStatus.port}/v1。`,
      );
    } catch (requestError) {
      setError(readError(requestError));
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
    setMessage(null);
    setError(null);
    try {
      const nextSettings = await updateSettings(formToInput(form));
      setSettings(nextSettings);
      setForm(settingsToForm(nextSettings));
      setMessage("设置已保存。");
    } catch (requestError) {
      setError(readError(requestError));
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
      <form id="settings-form" className="grid gap-3" onSubmit={handleSubmit}>
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
            description={`活动请求 ${proxyStatus.activeRequests} 个；${proxyStatus.lastError ?? "最近没有运行时错误"}`}
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

        <SectionCard title="路由与采集" description="P5 默认按 Key 池全局 priority fallback。">
          <SettingRow
            control={
              <select
                className={inputClassName}
                value={form.defaultRoutingStrategy}
                onChange={(event) =>
                  setForm({ ...form, defaultRoutingStrategy: event.target.value as RoutingStrategy })
                }
              >
                {Object.entries(routingStrategyLabels).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            }
            description="复杂策略仍是后续阶段；P5 使用 Key 池优先级。"
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
              <select
                className={inputClassName}
                value={form.trayBehavior}
                onChange={(event) =>
                  setForm({ ...form, trayBehavior: event.target.value as TrayBehavior })
                }
              >
                {Object.entries(trayBehaviorLabels).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
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
          <div className="rounded-2xl border border-amber-200 bg-amber-50/80 px-4 py-3 text-xs leading-5 text-amber-800">
            API Key 当前阶段暂存 SQLite 明文；P3/P4 前必须迁移到本地加密或系统密钥链。
          </div>
        </SectionCard>

        {(message || error) && (
          <div className={error ? "text-sm text-rose-700" : "text-sm text-emerald-700"}>
            {error ?? message}
          </div>
        )}
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
    <div className="grid min-h-14 grid-cols-[minmax(0,1fr)_260px] items-center gap-4 border-b border-cyan-100 py-3 last:border-b-0">
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
  "h-8 w-full rounded-xl border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
