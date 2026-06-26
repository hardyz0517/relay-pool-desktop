import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { Save } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button } from "@/components/ui/button";
import { KeyValueRow, MaskedSecret, SectionCard, StatusBadge } from "@/components/ui";
import { getSettings, updateSettings } from "@/lib/api/settings";
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

export function SettingsPage() {
  const [settings, setSettings] = useState<AppSettings>(fallbackSettings);
  const [form, setForm] = useState<SettingsFormState>(settingsToForm(fallbackSettings));
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
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
      setSettings(nextSettings);
      setForm(settingsToForm(nextSettings));
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setLoading(false);
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
      setMessage("设置已保存到本地 SQLite。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <PageScaffold
      eyebrow="Settings"
      title="设置"
      description="本地设置已部分接入 SQLite；当前仍不启动真实代理或采集器。"
    >
      <div className="grid gap-4 xl:grid-cols-2">
        <SectionCard
          title="本地代理"
          description="端口和本地 key 已来自持久化设置；代理服务仍未实现。"
          action={<StatusBadge tone={loading ? "disabled" : "info"}>本地 SQLite</StatusBadge>}
        >
          <dl>
            <KeyValueRow label="代理端口" value={settings.localProxyPort} />
            <KeyValueRow
              label="Base URL"
              value={`http://127.0.0.1:${settings.localProxyPort}/v1`}
            />
            <KeyValueRow
              label="Local Key"
              value={<MaskedSecret value={settings.localKeyMasked} />}
            />
          </dl>
          <div className="mt-3 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs text-amber-800">
            Local Key 当前保存在 settings 表；后续接入真实代理前需要迁移到本地加密或系统密钥链。
          </div>
        </SectionCard>

        <SectionCard title="可保存设置" description="这些字段刷新应用后会保留。">
          <form className="grid gap-3" onSubmit={handleSubmit}>
            <div className="grid gap-3 md:grid-cols-2">
              <Field label="本地代理端口">
                <input
                  className={inputClassName}
                  min="1"
                  max="65535"
                  type="number"
                  value={form.localProxyPort}
                  onChange={(event) =>
                    setForm({ ...form, localProxyPort: event.target.value })
                  }
                />
              </Field>
              <Field label="默认路由策略">
                <select
                  className={inputClassName}
                  value={form.defaultRoutingStrategy}
                  onChange={(event) =>
                    setForm({
                      ...form,
                      defaultRoutingStrategy: event.target.value as RoutingStrategy,
                    })
                  }
                >
                  {Object.entries(routingStrategyLabels).map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </Field>
            </div>

            <div className="grid gap-3 md:grid-cols-2">
              <Field label="低余额阈值">
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
              </Field>
              <Field label="采集频率（分钟）">
                <input
                  className={inputClassName}
                  min="1"
                  type="number"
                  value={form.collectorIntervalMinutes}
                  onChange={(event) =>
                    setForm({
                      ...form,
                      collectorIntervalMinutes: event.target.value,
                    })
                  }
                />
              </Field>
            </div>

            <Field label="托盘行为">
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
            </Field>

            <div className="flex flex-wrap items-center gap-2">
              <Button disabled={saving || loading} type="submit">
                <Save className="h-4 w-4" />
                {saving ? "保存中" : "保存设置"}
              </Button>
              {(message || error) && (
                <span
                  className={
                    error
                      ? "text-sm text-rose-700"
                      : "text-sm text-emerald-700"
                  }
                >
                  {error ?? message}
                </span>
              )}
            </div>
          </form>
        </SectionCard>

        <SectionCard title="数据与安全">
          <dl>
            <KeyValueRow label="数据目录" value={settings.dataDir} />
            <KeyValueRow
              label="API Key 存储"
              value="P2 暂存 SQLite 明文；P3/P4 前必须迁移到本地加密或系统密钥链。"
            />
          </dl>
        </SectionCard>

        <SectionCard title="导入 / 导出" description="Phase 2 仍只保留入口。">
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" disabled>
              导入配置
            </Button>
            <Button variant="outline" disabled>
              导出配置
            </Button>
            <Button variant="outline" disabled>
              打开数据目录
            </Button>
          </div>
          <div className="mt-3 rounded-md border border-border bg-slate-50 px-3 py-2 text-xs text-muted-foreground">
            不提交 key、cookie、日志、本地数据库或用户本地数据。
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
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
  "h-8 rounded-md border border-border bg-white px-2.5 text-sm text-slate-800 outline-none transition focus:border-blue-300 focus:ring-2 focus:ring-blue-100";
