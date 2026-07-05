import { useEffect, useState, type FormEvent } from "react";
import { ArrowLeft, Check, ShieldCheck } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, IconButton, PageForm, SectionCard, SelectControl, useToast } from "@/components/ui";
import { testStationLoginInput } from "@/lib/api/collector";
import { getStationCredentials, updateStationCredentials } from "@/lib/api/stationKeys";
import { createStation, listStations, updateStation } from "@/lib/api/stations";
import type { StationCredentials } from "@/lib/types/stationKeys";
import { stationTypeLabels, type Station, type StationType } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import { providerPresets, type ProviderPresetId } from "./providerPresets";

type AddProviderPageProps = {
  stationId?: string | null;
  onBack: () => void;
  onCreated?: () => void;
  onUpdated?: () => void;
};

type AddProviderFormState = {
  presetId: ProviderPresetId;
  name: string;
  stationType: StationType;
  baseUrl: string;
  apiKey: string;
  enabled: boolean;
  creditPerCny: string;
  loginUsername: string;
  loginPassword: string;
  rememberPassword: boolean;
  lowBalanceThresholdCny: string;
  note: string;
};

type ConnectionTestState = {
  status: "idle" | "testing" | "success" | "warning" | "error";
  message: string | null;
};

const defaultPreset = providerPresets[1];

const inputClassName =
  "h-8 rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]";

function formFromStation(station: Station, credentials: StationCredentials): AddProviderFormState {
  const preset = providerPresets.find((item) => item.stationType === station.stationType) ?? defaultPreset;
  return {
    presetId: preset.id,
    name: station.name,
    stationType: station.stationType,
    baseUrl: station.baseUrl,
    apiKey: "",
    enabled: station.enabled,
    creditPerCny: String(station.creditPerCny),
    loginUsername: credentials.loginUsername ?? "",
    loginPassword: "",
    rememberPassword: credentials.rememberPassword,
    lowBalanceThresholdCny:
      station.lowBalanceThresholdCny === null ? "" : String(station.lowBalanceThresholdCny),
    note: station.note ?? "",
  };
}

export function AddProviderPage({ stationId, onBack, onCreated, onUpdated }: AddProviderPageProps) {
  const toast = useToast();
  const editing = Boolean(stationId);
  const [form, setForm] = useState<AddProviderFormState>({
    presetId: defaultPreset.id,
    name: defaultPreset.name,
    stationType: defaultPreset.stationType,
    baseUrl: defaultPreset.baseUrl,
    apiKey: "",
    enabled: true,
    creditPerCny: "1",
    loginUsername: "",
    loginPassword: "",
    rememberPassword: false,
    lowBalanceThresholdCny: "",
    note: "",
  });
  const [loading, setLoading] = useState(Boolean(stationId));
  const [saving, setSaving] = useState(false);
  const [testingConnection, setTestingConnection] = useState(false);
  const [connectionTest, setConnectionTest] = useState<ConnectionTestState>({
    status: "idle",
    message: null,
  });
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!stationId) {
      setLoading(false);
      return;
    }

    let alive = true;
    setLoading(true);
    setError(null);
    void Promise.all([listStations(), getStationCredentials(stationId)])
      .then(([stations, credentials]) => {
        if (!alive) {
          return;
        }
        const station = stations.find((item) => item.id === stationId);
        if (!station) {
          throw new Error("未找到要编辑的供应商");
        }
        setForm(formFromStation(station, credentials));
        setConnectionTest({ status: "idle", message: null });
      })
      .catch((requestError) => {
        if (!alive) {
          return;
        }
        const message = readError(requestError);
        setError(message);
        toast.error("读取供应商失败", message);
      })
      .finally(() => {
        if (alive) {
          setLoading(false);
        }
      });

    return () => {
      alive = false;
    };
  }, [stationId, toast]);

  function applyPreset(presetId: ProviderPresetId) {
    const preset = providerPresets.find((item) => item.id === presetId) ?? defaultPreset;
    setForm((current) => ({
      ...current,
      presetId: preset.id,
      name: preset.name,
      stationType: preset.stationType,
      baseUrl: preset.baseUrl,
    }));
    setError(null);
    setConnectionTest({ status: "idle", message: null });
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!form.name.trim()) {
      toast.info("请填写供应商名称");
      return;
    }
    if (!form.baseUrl.trim()) {
      toast.info("请填写基础地址");
      return;
    }
    if (!editing && !form.apiKey.trim()) {
      toast.info("请填写密钥");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      if (stationId) {
        await updateStation({
          id: stationId,
          name: form.name.trim(),
          stationType: form.stationType,
          baseUrl: form.baseUrl.trim(),
          apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
          enabled: form.enabled,
          creditPerCny: Number(form.creditPerCny),
          lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim()
            ? Number(form.lowBalanceThresholdCny)
            : null,
          note: form.note.trim() ? form.note.trim() : null,
        });
        if (form.loginUsername.trim() || form.loginPassword.trim() || form.rememberPassword) {
          await updateStationCredentials({
            stationId,
            loginUsername: form.loginUsername.trim() ? form.loginUsername.trim() : null,
            loginPassword: form.loginPassword.trim() ? form.loginPassword.trim() : null,
            rememberPassword: form.rememberPassword,
          });
        }
        toast.success("供应商已更新");
        onUpdated?.();
        return;
      }

      const station = await createStation({
        name: form.name.trim(),
        stationType: form.stationType,
        baseUrl: form.baseUrl.trim(),
        apiKey: form.apiKey.trim(),
        enabled: form.enabled,
        creditPerCny: Number(form.creditPerCny),
        lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim()
          ? Number(form.lowBalanceThresholdCny)
          : null,
        note: form.note.trim() ? form.note.trim() : null,
      });
      if (form.loginUsername.trim() || form.loginPassword.trim()) {
        await updateStationCredentials({
          stationId: station.id,
          loginUsername: form.loginUsername.trim() ? form.loginUsername.trim() : null,
          loginPassword: form.loginPassword.trim() ? form.loginPassword.trim() : null,
          rememberPassword: Boolean(form.loginPassword.trim()),
        });
      }
      toast.success("供应商已添加");
      onCreated?.();
    } catch (requestError) {
      const message = requestError instanceof Error ? requestError.message : String(requestError);
      setError(message);
      toast.error("添加供应商失败", message);
    } finally {
      setSaving(false);
    }
  }

  async function handleTestConnection() {
    if (!form.baseUrl.trim()) {
      toast.info("请填写基础地址");
      return;
    }
    if (!form.loginUsername.trim() || !form.loginPassword.trim()) {
      toast.info("请填写登录用户名和密码");
      return;
    }

    setTestingConnection(true);
    setError(null);
    setConnectionTest({ status: "testing", message: "正在测试连通性..." });
    try {
      const result = await testStationLoginInput({
        baseUrl: form.baseUrl.trim(),
        loginUsername: form.loginUsername.trim(),
        loginPassword: form.loginPassword.trim(),
      });
      const message = result.diagnosis
        ? `${result.message} ${result.diagnosis}`
        : result.message;
      if (result.status === "success") {
        setConnectionTest({ status: "success", message });
        toast.success("连通性测试通过", result.message);
      } else {
        setConnectionTest({ status: "warning", message });
        toast.info("连通性测试已完成", result.message);
      }
    } catch (requestError) {
      const message = readError(requestError);
      setConnectionTest({ status: "error", message });
      toast.error("连通性测试失败", message);
    } finally {
      setTestingConnection(false);
    }
  }

  return (
    <PageScaffold
      title={editing ? "编辑供应商" : "添加新供应商"}
      stickyHeader
      backAction={
        <IconButton label="返回中转站" onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
    >
      <PageForm
        className="w-full"
        onSubmit={handleSubmit}
        footer={
          <>
            <Button variant="secondary" onClick={onBack} disabled={saving}>
              取消
            </Button>
            <Button type="submit" disabled={saving || loading}>
              <Check className="h-4 w-4" />
              {saving ? "保存中" : editing ? "保存修改" : "添加供应商"}
            </Button>
          </>
        }
      >
        <section className="grid gap-[var(--shell-page-gap)]">
          <div className="grid gap-[var(--shell-page-gap)]">
            {!editing && (
              <SectionCard title="预设供应商">
                <div className="grid grid-cols-[repeat(auto-fit,minmax(min(100%,9rem),1fr))] gap-2">
                  {providerPresets.map((preset) => {
                    const selected = preset.id === form.presetId;
                    return (
                      <button
                        key={preset.id}
                        type="button"
                        className={cn(
                          "relative flex h-8 min-w-0 cursor-pointer items-center gap-2 rounded-[var(--surface-radius)] px-2.5 text-left text-xs font-medium transition-colors",
                          selected
                            ? "bg-[hsl(var(--accent))] text-white shadow-sm"
                            : "bg-slate-100 text-slate-600 hover:bg-slate-200 hover:text-slate-900",
                        )}
                        onClick={() => applyPreset(preset.id)}
                        title={preset.description}
                      >
                        <span
                          className={cn(
                            "flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-[5px] bg-white text-[10px] font-semibold text-slate-600",
                            selected && "text-[hsl(var(--accent))]",
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
            )}

            <SectionCard title="连接信息">
              <div className="grid gap-3 md:grid-cols-2">
                <Field label="供应商名称">
                  <input
                    className={inputClassName}
                    value={form.name}
                    onChange={(event) => setForm({ ...form, name: event.target.value })}
                    placeholder="例如 DeepSeek"
                  />
                </Field>
                <Field label="站点类型">
                  <SelectControl
                    ariaLabel="站点类型"
                    className={inputClassName}
                    value={form.stationType}
                    options={Object.entries(stationTypeLabels).map(([value, label]) => ({
                      value: value as StationType,
                      label,
                    }))}
                    onChange={(stationType) => setForm({ ...form, stationType })}
                  />
                </Field>
              </div>
              <div className="mt-3 grid gap-3">
                <Field label="基础地址">
                  <input
                    className={inputClassName}
                    value={form.baseUrl}
                    onChange={(event) => {
                      setForm({ ...form, baseUrl: event.target.value });
                      setConnectionTest({ status: "idle", message: null });
                    }}
                    placeholder="https://api.example.com/v1"
                  />
                </Field>
                <Field label={editing ? "密钥" : "默认密钥"}>
                  <input
                    className={inputClassName}
                    type="password"
                    value={form.apiKey}
                    onChange={(event) => setForm({ ...form, apiKey: event.target.value })}
                    placeholder={editing ? "留空保留旧密钥" : "创建供应商时同步保存为默认密钥"}
                  />
                </Field>
              </div>
              <div className="mt-3 grid gap-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] md:items-end">
                <Field label="登录用户名 / 邮箱">
                  <input
                    className={inputClassName}
                    value={form.loginUsername}
                    onChange={(event) => {
                      setForm({ ...form, loginUsername: event.target.value });
                      setConnectionTest({ status: "idle", message: null });
                    }}
                    placeholder="user@example.com"
                  />
                </Field>
                <Field label="登录密码">
                  <input
                    className={inputClassName}
                    type="password"
                    value={form.loginPassword}
                    onChange={(event) => {
                      setForm({
                        ...form,
                        loginPassword: event.target.value,
                        rememberPassword: Boolean(event.target.value.trim()),
                      });
                      setConnectionTest({ status: "idle", message: null });
                    }}
                    placeholder={editing ? "留空保留旧密码" : "用于采集登录"}
                  />
                </Field>
                <Button
                  variant="outline"
                  onClick={handleTestConnection}
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
                    connectionTest.status === "success" && "text-emerald-600",
                    connectionTest.status === "warning" && "text-amber-600",
                    connectionTest.status === "error" && "text-rose-600",
                    connectionTest.status === "testing" && "text-slate-500",
                  )}
                >
                  {connectionTest.message}
                </div>
              )}
              {error && (
                <div className="mt-3 rounded-[var(--surface-radius)] border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-700">
                  {error}
                </div>
              )}
            </SectionCard>
          </div>

          <aside className="grid content-start gap-[var(--shell-page-gap)]">
            <SectionCard title="可选项">
              <div className="grid gap-3">
                <Field label="低余额阈值 CNY">
                  <input
                    className={inputClassName}
                    min="0"
                    step="0.01"
                    type="number"
                    value={form.lowBalanceThresholdCny}
                    onChange={(event) => setForm({ ...form, lowBalanceThresholdCny: event.target.value })}
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
                    onChange={(event) => setForm({ ...form, creditPerCny: event.target.value })}
                  />
                </Field>
                <Field label="备注">
                  <textarea
                    className={`${inputClassName} min-h-24 resize-none py-2`}
                    value={form.note}
                    onChange={(event) => setForm({ ...form, note: event.target.value })}
                    placeholder="登录方式、模型限制或计费说明"
                  />
                </Field>
              </div>
            </SectionCard>
          </aside>
        </section>
      </PageForm>
    </PageScaffold>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
