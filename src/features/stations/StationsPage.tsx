import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { ArrowDown, ArrowUp, Plus, Save } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, EmptyState, SectionCard } from "@/components/ui";
import {
  createStation,
  deleteStation,
  listStations,
  reorderStations,
  updateStation,
} from "@/lib/api/stations";
import {
  stationTypeLabels,
  type Station,
  type StationInput,
  type StationType,
} from "@/lib/types/stations";
import { StationDetailPanel } from "./components/StationDetailPanel";
import { StationListItem } from "./components/StationListItem";

type StationFormState = {
  name: string;
  stationType: StationType;
  baseUrl: string;
  apiKey: string;
  enabled: boolean;
  creditPerCny: string;
  lowBalanceThresholdCny: string;
  note: string;
};

const emptyForm: StationFormState = {
  name: "",
  stationType: "sub2api",
  baseUrl: "",
  apiKey: "",
  enabled: true,
  creditPerCny: "1",
  lowBalanceThresholdCny: "",
  note: "",
};

const exampleStation: StationFormState = {
  name: "Orchid Relay",
  stationType: "sub2api",
  baseUrl: "https://api.orchid-relay.example/v1",
  apiKey: "sk-example-change-me",
  enabled: true,
  creditPerCny: "1",
  lowBalanceThresholdCny: "15",
  note: "示例站点，仅用于验证本地 SQLite 持久化。",
};

export function StationsPage() {
  const [stations, setStations] = useState<Station[]>([]);
  const [selectedStationId, setSelectedStationId] = useState<string | null>(null);
  const [editingStationId, setEditingStationId] = useState<string | null>(null);
  const [form, setForm] = useState<StationFormState>(emptyForm);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refreshStations();
  }, []);

  const selectedStation = useMemo(
    () => stations.find((station) => station.id === selectedStationId) ?? stations[0],
    [selectedStationId, stations],
  );

  async function refreshStations() {
    setLoading(true);
    setError(null);
    try {
      const nextStations = await listStations();
      setStations(nextStations);
      setSelectedStationId((current) => {
        if (current && nextStations.some((station) => station.id === current)) {
          return current;
        }
        return nextStations[0]?.id ?? null;
      });
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setLoading(false);
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaving(true);
    setError(null);
    setMessage(null);

    try {
      const input = formToInput(form);
      const station = editingStationId
        ? await updateStation({
            ...input,
            id: editingStationId,
            apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
          })
        : await createStation(input);

      await refreshStations();
      setSelectedStationId(station.id);
      setEditingStationId(null);
      setForm(emptyForm);
      setMessage(editingStationId ? "站点已更新。" : "站点已创建并保存到本地 SQLite。");
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete(station: Station) {
    if (!window.confirm(`确认删除站点「${station.name}」？`)) {
      return;
    }

    setError(null);
    setMessage(null);
    try {
      await deleteStation(station.id);
      await refreshStations();
      setEditingStationId(null);
      setForm(emptyForm);
      setMessage("站点已删除。");
    } catch (requestError) {
      setError(readError(requestError));
    }
  }

  async function handleToggleEnabled(station: Station) {
    setError(null);
    setMessage(null);
    try {
      await updateStation({
        id: station.id,
        name: station.name,
        stationType: station.stationType,
        baseUrl: station.baseUrl,
        apiKey: null,
        enabled: !station.enabled,
        creditPerCny: station.creditPerCny,
        lowBalanceThresholdCny: station.lowBalanceThresholdCny,
        note: station.note,
      });
      await refreshStations();
      setMessage(station.enabled ? "站点已禁用。" : "站点已启用。");
    } catch (requestError) {
      setError(readError(requestError));
    }
  }

  async function moveStation(station: Station, direction: -1 | 1) {
    const index = stations.findIndex((item) => item.id === station.id);
    const targetIndex = index + direction;
    if (index < 0 || targetIndex < 0 || targetIndex >= stations.length) {
      return;
    }

    const nextStations = [...stations];
    const [movedStation] = nextStations.splice(index, 1);
    nextStations.splice(targetIndex, 0, movedStation);

    setStations(nextStations);
    setError(null);
    setMessage(null);

    try {
      const savedStations = await reorderStations(nextStations.map((item) => item.id));
      setStations(savedStations);
      setSelectedStationId(station.id);
      setMessage("站点排序已保存。");
    } catch (requestError) {
      setError(readError(requestError));
      await refreshStations();
    }
  }

  function startCreate(prefill?: StationFormState) {
    setEditingStationId(null);
    setForm(prefill ?? emptyForm);
    setMessage(null);
    setError(null);
  }

  function startEdit(station: Station) {
    setEditingStationId(station.id);
    setForm({
      name: station.name,
      stationType: station.stationType,
      baseUrl: station.baseUrl,
      apiKey: "",
      enabled: station.enabled,
      creditPerCny: String(station.creditPerCny),
      lowBalanceThresholdCny:
        station.lowBalanceThresholdCny === null
          ? ""
          : String(station.lowBalanceThresholdCny),
      note: station.note ?? "",
    });
    setMessage(null);
    setError(null);
  }

  return (
    <PageScaffold
      eyebrow="Stations"
      title="中转池"
      description="站点基础信息已接入本地 SQLite；采集、检测和真实连接仍为后续阶段。"
    >
      <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
        <div className="space-y-3">
          <SectionCard
            title="站点列表"
            description="排序、启用状态和基础信息会保存到本地数据库。"
            action={
              <Button variant="outline" onClick={() => startCreate()}>
                <Plus className="h-4 w-4" />
                新增
              </Button>
            }
            contentClassName="space-y-2"
          >
            {loading ? (
              <div className="rounded-md border border-border bg-slate-50 px-3 py-2 text-sm text-muted-foreground">
                正在读取本地 SQLite...
              </div>
            ) : stations.length === 0 ? (
              <EmptyState
                title="还没有中转站"
                description="可以先添加一个示例站点，用来验证本地 SQLite 持久化。"
                action={
                  <Button variant="outline" onClick={() => startCreate(exampleStation)}>
                    添加示例站点
                  </Button>
                }
              />
            ) : (
              stations.map((station, index) => (
                <div key={station.id} className="flex gap-2">
                  <div className="min-w-0 flex-1">
                    <StationListItem
                      station={station}
                      active={station.id === selectedStation?.id}
                      onSelect={() => setSelectedStationId(station.id)}
                    />
                  </div>
                  <div className="flex flex-col gap-1">
                    <Button
                      variant="ghost"
                      className="h-7 w-7 px-0"
                      disabled={index === 0}
                      onClick={() => moveStation(station, -1)}
                      title="上移"
                    >
                      <ArrowUp className="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      variant="ghost"
                      className="h-7 w-7 px-0"
                      disabled={index === stations.length - 1}
                      onClick={() => moveStation(station, 1)}
                      title="下移"
                    >
                      <ArrowDown className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </div>
              ))
            )}
          </SectionCard>

          {(message || error) && (
            <div
              className={
                error
                  ? "rounded-md border border-rose-200 bg-rose-50 px-3 py-2 text-sm text-rose-700"
                  : "rounded-md border border-emerald-200 bg-emerald-50 px-3 py-2 text-sm text-emerald-700"
              }
            >
              {error ?? message}
            </div>
          )}
        </div>

        <div className="space-y-4">
          <StationForm
            editing={Boolean(editingStationId)}
            form={form}
            saving={saving}
            onCancel={() => {
              setEditingStationId(null);
              setForm(emptyForm);
            }}
            onChange={setForm}
            onSubmit={handleSubmit}
          />

          {selectedStation ? (
            <StationDetailPanel
              station={selectedStation}
              onEdit={() => startEdit(selectedStation)}
              onDelete={() => handleDelete(selectedStation)}
              onToggleEnabled={() => handleToggleEnabled(selectedStation)}
            />
          ) : (
            <EmptyState
              title="未选择站点"
              description="新增站点后，这里会显示持久化后的详情。"
            />
          )}
        </div>
      </div>
    </PageScaffold>
  );
}

function StationForm({
  editing,
  form,
  saving,
  onCancel,
  onChange,
  onSubmit,
}: {
  editing: boolean;
  form: StationFormState;
  saving: boolean;
  onCancel: () => void;
  onChange: (nextForm: StationFormState) => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <SectionCard
      title={editing ? "编辑站点" : "添加中转站"}
      description="API Key 当前会写入本地 SQLite；后续 P3/P4 前需要迁移到本地加密或系统密钥链。"
    >
      <form className="grid gap-3" onSubmit={onSubmit}>
        <div className="grid gap-3 md:grid-cols-2">
          <Field label="站点名称">
            <input
              className={inputClassName}
              value={form.name}
              onChange={(event) => onChange({ ...form, name: event.target.value })}
              placeholder="例如 Orchid Relay"
              required
            />
          </Field>
          <Field label="站点类型">
            <select
              className={inputClassName}
              value={form.stationType}
              onChange={(event) =>
                onChange({ ...form, stationType: event.target.value as StationType })
              }
            >
              {Object.entries(stationTypeLabels).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </Field>
        </div>

        <Field label="Base URL">
          <input
            className={inputClassName}
            value={form.baseUrl}
            onChange={(event) => onChange({ ...form, baseUrl: event.target.value })}
            placeholder="https://example.com/v1"
            required
          />
        </Field>

        <Field label={editing ? "API Key（留空保持不变）" : "API Key"}>
          <input
            className={inputClassName}
            value={form.apiKey}
            onChange={(event) => onChange({ ...form, apiKey: event.target.value })}
            placeholder={editing ? "留空保持原 key" : "sk-..."}
            required={!editing}
          />
        </Field>

        <div className="grid gap-3 md:grid-cols-3">
          <Field label="兑换比例">
            <input
              className={inputClassName}
              min="0.01"
              step="0.01"
              type="number"
              value={form.creditPerCny}
              onChange={(event) =>
                onChange({ ...form, creditPerCny: event.target.value })
              }
            />
          </Field>
          <Field label="低余额阈值">
            <input
              className={inputClassName}
              min="0"
              step="0.01"
              type="number"
              value={form.lowBalanceThresholdCny}
              onChange={(event) =>
                onChange({ ...form, lowBalanceThresholdCny: event.target.value })
              }
              placeholder="使用全局"
            />
          </Field>
          <label className="flex items-end gap-2 pb-2 text-sm text-slate-700">
            <input
              checked={form.enabled}
              className="h-4 w-4"
              type="checkbox"
              onChange={(event) => onChange({ ...form, enabled: event.target.checked })}
            />
            启用站点
          </label>
        </div>

        <Field label="备注">
          <textarea
            className={`${inputClassName} min-h-16 resize-none py-2`}
            value={form.note}
            onChange={(event) => onChange({ ...form, note: event.target.value })}
            placeholder="本地备注，不会用于真实请求"
          />
        </Field>

        <div className="flex flex-wrap gap-2">
          <Button disabled={saving} type="submit">
            <Save className="h-4 w-4" />
            {saving ? "保存中" : editing ? "保存修改" : "保存站点"}
          </Button>
          {editing && (
            <Button variant="outline" onClick={onCancel}>
              取消编辑
            </Button>
          )}
        </div>
      </form>
    </SectionCard>
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

function formToInput(form: StationFormState): StationInput {
  return {
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
  };
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const inputClassName =
  "h-8 rounded-md border border-border bg-white px-2.5 text-sm text-slate-800 outline-none transition focus:border-blue-300 focus:ring-2 focus:ring-blue-100";
