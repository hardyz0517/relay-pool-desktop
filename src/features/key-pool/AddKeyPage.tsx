import { useEffect, useState, type FormEvent } from "react";
import { ArrowLeft, Check } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, IconButton, PageForm, SectionCard, SelectControl, useToast } from "@/components/ui";
import { listStationGroupOptions } from "@/lib/api/groupFacts";
import { listStations } from "@/lib/api/stations";
import { saveStationKeyWithDefaults } from "@/lib/api/stationKeys";
import { readError } from "@/lib/errors";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { Station } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import { formatStationGroupOptionLabel } from "@/features/stations/groupOptionViewModels";

type AddKeyPageProps = {
  initialStationId?: string | null;
  onBack: () => void;
  onCreated?: () => void;
};

type AddKeyFormState = {
  stationId: string;
  name: string;
  baseUrl: string;
  apiKey: string;
  priority: string;
  groupBindingId: string;
  groupName: string;
  tierLabel: string;
  note: string;
};

const emptyForm: AddKeyFormState = {
  stationId: "",
  name: "",
  baseUrl: "",
  apiKey: "",
  priority: "0",
  groupBindingId: "",
  groupName: "",
  tierLabel: "",
  note: "",
};

const inputClassName =
  "h-8 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30";

export function AddKeyPage({ initialStationId, onBack, onCreated }: AddKeyPageProps) {
  const toast = useToast();
  const [stations, setStations] = useState<Station[]>([]);
  const [groupOptions, setGroupOptions] = useState<StationGroupOption[]>([]);
  const [form, setForm] = useState<AddKeyFormState>(emptyForm);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const bindingOptions = groupOptions
    .filter((option) => option.groupBindingId)
    .map((option) => ({
      value: option.groupBindingId ?? option.value,
      label: groupOptionLabel(option),
    }));

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError(null);
    void listStations()
      .then((nextStations) => {
        if (!alive) return;
        setStations(nextStations);
        const station = initialStationId
          ? nextStations.find((item) => item.id === initialStationId) ?? null
          : null;
        if (station) {
          setForm(createFormForStation(station));
          void refreshGroupOptions(station.id, alive);
        } else {
          setForm(emptyForm);
          setGroupOptions([]);
        }
      })
      .catch((requestError) => {
        if (!alive) return;
        const message = readError(requestError);
        setError(message);
        toast.error("读取中转站失败", message);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [initialStationId, toast]);

  async function refreshGroupOptions(stationId: string, alive = true) {
    try {
      const nextOptions = await listStationGroupOptions(stationId);
      if (alive) setGroupOptions(nextOptions);
    } catch (requestError) {
      if (alive) toast.error("读取中转站分组失败", readError(requestError));
    }
  }

  function selectStation(station: Station) {
    setForm(createFormForStation(station));
    setGroupOptions([]);
    void refreshGroupOptions(station.id);
  }

  function selectCustomConfig() {
    setForm(emptyForm);
    setGroupOptions([]);
  }

  function handleStationChange(stationId: string) {
    const station = stations.find((item) => item.id === stationId);
    setForm((current) => ({
      ...current,
      stationId,
      baseUrl: station?.baseUrl ?? "",
      priority: station ? String(station.keyCount) : "0",
      groupBindingId: "",
      groupName: "",
      tierLabel: "",
    }));
    setGroupOptions([]);
    if (station) {
      void refreshGroupOptions(station.id);
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!form.stationId) {
      toast.info("请选择中转站");
      return;
    }
    if (!form.apiKey.trim()) {
      toast.info("请填写密钥");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      const groupOption = selectedGroupOption(groupOptions, form.groupBindingId);
      await saveStationKeyWithDefaults({
        mode: "create",
        stationId: form.stationId,
        name: form.name.trim(),
        apiKey: form.apiKey.trim(),
        enabled: true,
        priority: Number(form.priority),
        tierLabel: form.tierLabel.trim() ? form.tierLabel.trim() : null,
        note: form.note.trim() ? form.note.trim() : null,
        groupSelection: groupOption?.groupBindingId
          ? {
              kind: "set",
              groupBindingId: groupOption.groupBindingId,
              groupIdHash: groupOption.groupIdHash,
              groupName: groupOption.groupName,
            }
          : { kind: "clear" },
      });
      toast.success("密钥已添加");
      onCreated?.();
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("添加密钥失败", message);
    } finally {
      setSaving(false);
    }
  }

  return (
    <PageScaffold
      title="添加密钥"
      stickyHeader
      backAction={
        <IconButton label="返回密钥池" onClick={onBack}>
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
            <Button type="submit" disabled={saving || loading || stations.length === 0}>
              <Check className="h-4 w-4" />
              {saving ? "保存中" : "添加密钥"}
            </Button>
          </>
        }
      >
        <section className="grid gap-[var(--shell-page-gap)]">
          <div className="grid gap-[var(--shell-page-gap)]">
            <SectionCard title="预设中转站">
              {stations.length === 0 ? (
                <div className="space-y-2">
                  <PresetButton
                    label="自定义配置"
                    selected
                    onClick={selectCustomConfig}
                  />
                  <div className="rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2 text-sm text-muted-foreground">
                    还没有可用中转站，请先添加供应商。
                  </div>
                </div>
              ) : (
                <div className="flex flex-wrap gap-2">
                  <PresetButton
                    label="自定义配置"
                    selected={!form.stationId}
                    onClick={selectCustomConfig}
                  />
                  {stations.map((station) => {
                    const selected = station.id === form.stationId;
                    return (
                      <PresetButton
                        key={station.id}
                        label={station.name}
                        selected={selected}
                        onClick={() => selectStation(station)}
                        title={station.baseUrl}
                      />
                    );
                  })}
                </div>
              )}
            </SectionCard>

            <SectionCard title="密钥信息">
              <div className="grid gap-3 md:grid-cols-2">
                <Field label="所属中转站">
                  <SelectControl
                    ariaLabel="所属中转站"
                    className={inputClassName}
                    value={form.stationId}
                    options={[
                      { value: "", label: "请选择中转站" },
                      ...stations.map((station) => ({ value: station.id, label: station.name })),
                    ]}
                    onChange={handleStationChange}
                  />
                </Field>
                <Field label="名称">
                  <input className={inputClassName} value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} required />
                </Field>
              </div>
              <div className="mt-3 grid gap-3">
                <Field label="Base URL">
                  <input
                    className={inputClassName}
                    value={form.baseUrl}
                    onChange={(event) => setForm({ ...form, baseUrl: event.target.value })}
                    placeholder="https://api.example.com/v1"
                  />
                </Field>
                <Field label="密钥">
                  <input className={inputClassName} type="password" value={form.apiKey} onChange={(event) => setForm({ ...form, apiKey: event.target.value })} placeholder="sk-..." required />
                </Field>
              </div>
              {error && (
                <div className="mt-3 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
                  {error}
                </div>
              )}
            </SectionCard>
          </div>

          <aside className="grid content-start gap-[var(--shell-page-gap)]">
            <SectionCard title="可选项">
              <div className="grid gap-3">
                <Field label="分组绑定">
                  <SelectControl
                    ariaLabel="分组绑定"
                    className={inputClassName}
                    value={form.groupBindingId}
                    options={[
                      { value: "", label: bindingOptions.length ? "不绑定分组" : "暂无可用分组" },
                      ...bindingOptions,
                    ]}
                    onChange={(groupBindingId) => {
                      const groupOption = selectedGroupOption(groupOptions, groupBindingId);
                      setForm({
                        ...form,
                        groupBindingId,
                        groupName: groupOption?.groupName ?? "",
                      });
                    }}
                  />
                </Field>
                <div className="grid gap-3 md:grid-cols-2">
                  <Field label="优先级">
                    <input className={inputClassName} type="number" value={form.priority} onChange={(event) => setForm({ ...form, priority: event.target.value })} />
                  </Field>
                  <Field label="分组（随绑定同步）">
                    <input
                      className={`${inputClassName} bg-surface-subtle text-muted-foreground`}
                      value={form.groupName}
                      placeholder="选择分组绑定后自动填充"
                      readOnly
                    />
                  </Field>
                </div>
                <Field label="档位">
                  <input className={inputClassName} value={form.tierLabel} onChange={(event) => setForm({ ...form, tierLabel: event.target.value })} />
                </Field>
                <Field label="备注">
                  <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.note} onChange={(event) => setForm({ ...form, note: event.target.value })} />
                </Field>
              </div>
            </SectionCard>
          </aside>
        </section>
      </PageForm>
    </PageScaffold>
  );
}

function PresetButton({
  label,
  onClick,
  selected,
  title,
}: {
  label: string;
  onClick: () => void;
  selected: boolean;
  title?: string;
}) {
  return (
    <button
      type="button"
      className={cn(
        "relative flex h-8 w-[10rem] min-w-0 cursor-pointer items-center gap-2 rounded-[var(--surface-radius)] px-2.5 text-left text-xs font-medium transition-colors",
        selected
          ? "bg-primary-solid text-primary-foreground shadow-sm"
          : "bg-muted text-muted-foreground hover:bg-hover hover:text-foreground",
      )}
      onClick={onClick}
      title={title}
    >
      <span
        className={cn(
          "flex h-4.5 w-4.5 shrink-0 items-center justify-center rounded-[5px] bg-surface text-[10px] font-semibold text-muted-foreground",
          selected && "text-primary",
        )}
      >
        {label.slice(0, 1)}
      </span>
      <span className="min-w-0 truncate">{label}</span>
      {selected && <Check className="ml-auto h-3.5 w-3.5 shrink-0" />}
    </button>
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

function createFormForStation(station: Station): AddKeyFormState {
  return {
    ...emptyForm,
    stationId: station.id,
    baseUrl: station.baseUrl,
    name: `${station.name} Key`,
  };
}

function selectedGroupOption(options: StationGroupOption[], value: string) {
  return options.find((option) => option.groupBindingId === value || option.value === value) ?? null;
}

function groupOptionLabel(option: StationGroupOption) {
  return formatStationGroupOptionLabel(option);
}
