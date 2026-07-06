import { useEffect, useState, type FormEvent } from "react";
import { ArrowLeft, Check, KeyRound } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, EmptyState, IconButton, PageForm, SectionCard, SelectControl, useToast } from "@/components/ui";
import { listStationGroupOptions } from "@/lib/api/groupFacts";
import { listKeyPoolItems, saveStationKeyWithDefaults } from "@/lib/api/stationKeys";
import { readError } from "@/lib/errors";
import { formatRate } from "@/lib/formatters";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { KeyPoolItem, StationKeyStatus } from "@/lib/types/stationKeys";

type EditKeyPageProps = {
  stationKeyId: string | null;
  onBack: () => void;
  onUpdated?: () => void;
};

type EditKeyFormState = {
  id: string;
  stationId: string;
  stationName: string;
  stationBaseUrl: string;
  name: string;
  apiKey: string;
  enabled: boolean;
  priority: string;
  groupBindingId: string;
  groupName: string;
  tierLabel: string;
  status: StationKeyStatus;
  note: string;
  modelAllowlist: string;
  modelBlocklist: string;
  preferredModels: string;
  onlyUseAsBackup: boolean;
  routingTags: string;
};

const emptyForm: EditKeyFormState = {
  id: "",
  stationId: "",
  stationName: "",
  stationBaseUrl: "",
  name: "",
  apiKey: "",
  enabled: true,
  priority: "0",
  groupBindingId: "",
  groupName: "",
  tierLabel: "",
  status: "unchecked",
  note: "",
  modelAllowlist: "",
  modelBlocklist: "",
  preferredModels: "",
  onlyUseAsBackup: false,
  routingTags: "",
};

const inputClassName =
  "h-8 rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)] disabled:bg-slate-50 disabled:text-slate-500";

const KEEP_GROUP_BINDING_VALUE = "__keep__";
const CLEAR_GROUP_BINDING_VALUE = "__clear__";

export function EditKeyPage({ stationKeyId, onBack, onUpdated }: EditKeyPageProps) {
  const toast = useToast();
  const [sourceItem, setSourceItem] = useState<KeyPoolItem | null>(null);
  const [groupOptions, setGroupOptions] = useState<StationGroupOption[]>([]);
  const [form, setForm] = useState<EditKeyFormState>(emptyForm);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const bindingOptions = [
    ...groupOptions
      .filter((option) => option.groupBindingId)
      .map((option) => ({
        value: option.groupBindingId ?? option.value,
        label: groupOptionLabel(option),
      })),
    ...currentGroupOption(sourceItem, groupOptions),
  ];

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError(null);
    setSourceItem(null);
    setGroupOptions([]);
    setForm(emptyForm);

    if (!stationKeyId) {
      setLoading(false);
      setError("未选择要编辑的密钥。");
      return () => {
        alive = false;
      };
    }

    void listKeyPoolItems()
      .then(async (items) => {
        if (!alive) {
          return;
        }
        const item = items.find((candidate) => candidate.id === stationKeyId) ?? null;
        if (!item) {
          throw new Error("未找到要编辑的密钥。");
        }
        const nextGroupOptions = await listStationGroupOptions(item.stationId);
        if (!alive) {
          return;
        }
        setSourceItem(item);
        setGroupOptions(nextGroupOptions);
        setForm(formFromItem(item));
      })
      .catch((requestError) => {
        if (!alive) {
          return;
        }
        const message = readError(requestError);
        setError(message);
        toast.error("读取密钥详情失败", message);
      })
      .finally(() => {
        if (alive) {
          setLoading(false);
        }
      });

    return () => {
      alive = false;
    };
  }, [stationKeyId, toast]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!sourceItem) {
      return;
    }

    setSaving(true);
    setError(null);
    try {
      await saveStationKeyWithDefaults({
        mode: "update",
        id: form.id,
        stationId: form.stationId,
        name: form.name.trim(),
        apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
        enabled: form.enabled,
        priority: Number(form.priority),
        tierLabel: form.tierLabel.trim() ? form.tierLabel.trim() : null,
        balanceScope: sourceItem.balanceScope,
        status: form.status,
        note: form.note.trim() ? form.note.trim() : null,
        groupSelection: groupSelectionFromEditForm(form, sourceItem, groupOptions),
        capabilities: {
          stationKeyId: form.id,
          supportsChatCompletions: true,
          supportsResponses: true,
          supportsEmbeddings: true,
          supportsStream: true,
          supportsTools: true,
          supportsVision: true,
          supportsReasoning: true,
          modelAllowlist: linesToList(form.modelAllowlist),
          modelBlocklist: linesToList(form.modelBlocklist),
          preferredModels: linesToList(form.preferredModels),
          onlyUseAsBackup: form.onlyUseAsBackup,
          routingTags: commaListToList(form.routingTags),
        },
      });
      toast.success("密钥已更新");
      onUpdated?.();
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("保存密钥失败", message);
    } finally {
      setSaving(false);
    }
  }

  return (
    <PageScaffold
      title="编辑密钥"
      stickyHeader
      backAction={
        <IconButton label="返回密钥池" onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
      status={
        sourceItem ? (
          <span className="inline-flex h-6 items-center gap-1 rounded-[var(--surface-radius)] border border-border bg-white px-2 text-xs font-medium text-slate-600">
            <KeyRound className="h-3.5 w-3.5" />
            {sourceItem.apiKeyMasked}
          </span>
        ) : undefined
      }
    >
      {loading ? (
        <div className="rounded-[var(--surface-radius)] border border-border bg-white px-4 py-5 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
          正在读取密钥详情...
        </div>
      ) : !sourceItem ? (
        <EmptyState title="未找到密钥" description={error ?? "请回到密钥池重新选择要编辑的密钥。"} />
      ) : (
        <PageForm
          className="w-full"
          onSubmit={handleSubmit}
          footer={
            <>
              <Button variant="secondary" onClick={onBack} disabled={saving}>
                取消
              </Button>
              <Button type="submit" disabled={saving}>
                <Check className="h-4 w-4" />
                {saving ? "保存中..." : "保存密钥"}
              </Button>
            </>
          }
        >
          <section className="grid gap-[var(--shell-page-gap)]">
            <div className="grid gap-[var(--shell-page-gap)]">
              <SectionCard title="密钥信息">
                <div className="grid gap-3 md:grid-cols-2">
                  <Field label="所属中转站">
                    <input className={inputClassName} value={form.stationName} disabled />
                  </Field>
                  <Field label="名称">
                    <input
                      className={inputClassName}
                      value={form.name}
                      onChange={(event) => setForm({ ...form, name: event.target.value })}
                      required
                    />
                  </Field>
                </div>
                <div className="mt-3 grid gap-3">
                  <Field label="Base URL">
                    <input className={inputClassName} value={form.stationBaseUrl} disabled />
                  </Field>
                  <Field label="密钥">
                    <input
                      className={inputClassName}
                      type="password"
                      value={form.apiKey}
                      onChange={(event) => setForm({ ...form, apiKey: event.target.value })}
                      placeholder="留空保留旧密钥"
                    />
                  </Field>
                </div>
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
                  <Field label="分组绑定">
                    <SelectControl
                      ariaLabel="分组绑定"
                      className={inputClassName}
                      value={form.groupBindingId}
                      options={[
                        { value: KEEP_GROUP_BINDING_VALUE, label: "不调整绑定" },
                        ...(sourceItem?.groupBindingId ? [{ value: CLEAR_GROUP_BINDING_VALUE, label: "清除绑定" }] : []),
                        ...bindingOptions,
                      ]}
                      onChange={(groupBindingId) => {
                        setForm({
                          ...form,
                          groupBindingId,
                          groupName: groupNameForEditSelection(groupBindingId, sourceItem, groupOptions, form.groupName),
                        });
                      }}
                    />
                  </Field>
                  <div className="grid gap-3 md:grid-cols-2">
                    <Field label="优先级">
                      <input className={inputClassName} type="number" value={form.priority} onChange={(event) => setForm({ ...form, priority: event.target.value })} />
                    </Field>
                    <Field label="状态">
                      <SelectControl
                        ariaLabel="密钥状态"
                        className={inputClassName}
                        value={form.status}
                        options={[
                          { value: "unchecked", label: "未检测" },
                          { value: "healthy", label: "正常" },
                          { value: "warning", label: "警告" },
                          { value: "error", label: "错误" },
                          { value: "disabled", label: "禁用" },
                        ]}
                        onChange={(status) => setForm({ ...form, status })}
                      />
                    </Field>
                  </div>
                  <div className="grid gap-3 md:grid-cols-2">
                    <Field label="分组（随绑定同步）">
                      <input
                        className={`${inputClassName} bg-slate-50 text-slate-500`}
                        value={form.groupName}
                        placeholder="选择分组绑定后自动填充"
                        readOnly
                      />
                    </Field>
                    <Field label="档位">
                      <input className={inputClassName} value={form.tierLabel} onChange={(event) => setForm({ ...form, tierLabel: event.target.value })} />
                    </Field>
                  </div>
                  <CheckField label="启用" checked={form.enabled} onChange={(checked) => setForm({ ...form, enabled: checked })} />
                  <CheckField label="仅作为备用密钥" checked={form.onlyUseAsBackup} onChange={(checked) => setForm({ ...form, onlyUseAsBackup: checked })} />
                  <Field label="路由标签">
                    <input className={inputClassName} value={form.routingTags} onChange={(event) => setForm({ ...form, routingTags: event.target.value })} placeholder="逗号分隔，例如：高优先级, 低延迟" />
                  </Field>
                  <Field label="备注">
                    <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.note} onChange={(event) => setForm({ ...form, note: event.target.value })} />
                  </Field>
                </div>
              </SectionCard>

              <SectionCard title="模型范围">
                <div className="grid gap-3">
                  <Field label="允许模型">
                    <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.modelAllowlist} onChange={(event) => setForm({ ...form, modelAllowlist: event.target.value })} placeholder="每行一个模型；留空表示全部模型" />
                  </Field>
                  <Field label="禁止模型">
                    <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.modelBlocklist} onChange={(event) => setForm({ ...form, modelBlocklist: event.target.value })} placeholder="每行一个模型" />
                  </Field>
                  <Field label="优先模型">
                    <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.preferredModels} onChange={(event) => setForm({ ...form, preferredModels: event.target.value })} placeholder="每行一个模型" />
                  </Field>
                </div>
              </SectionCard>
            </aside>
          </section>
        </PageForm>
      )}
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

function CheckField({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center gap-2 text-sm text-slate-700">
      <input
        checked={checked}
        className="h-4 w-4 accent-teal-600"
        type="checkbox"
        onChange={(event) => onChange(event.target.checked)}
      />
      {label}
    </label>
  );
}

function formFromItem(item: KeyPoolItem): EditKeyFormState {
  return {
    id: item.id,
    stationId: item.stationId,
    stationName: item.stationName,
    stationBaseUrl: item.stationBaseUrl,
    name: item.name,
    apiKey: "",
    enabled: item.enabled,
    priority: String(item.priority),
    groupBindingId: KEEP_GROUP_BINDING_VALUE,
    groupName: item.groupName ?? "",
    tierLabel: item.tierLabel ?? "",
    status: item.status,
    note: item.note ?? "",
    modelAllowlist: "",
    modelBlocklist: "",
    preferredModels: "",
    onlyUseAsBackup: item.onlyUseAsBackup,
    routingTags: "",
  };
}

function groupSelectionFromEditForm(
  form: EditKeyFormState,
  sourceItem: KeyPoolItem,
  options: StationGroupOption[],
) {
  if (
    !form.groupBindingId ||
    form.groupBindingId === KEEP_GROUP_BINDING_VALUE ||
    form.groupBindingId === sourceItem.groupBindingId
  ) {
    return { kind: "keep" as const };
  }
  if (form.groupBindingId === CLEAR_GROUP_BINDING_VALUE) {
    return { kind: "clear" as const };
  }
  const groupOption = selectedGroupOption(options, form.groupBindingId);
  return {
    kind: "set" as const,
    groupBindingId: groupOption?.groupBindingId ?? form.groupBindingId,
    groupIdHash: groupOption?.groupIdHash ?? null,
    groupName: groupOption?.groupName ?? null,
  };
}

function selectedGroupOption(options: StationGroupOption[], value: string) {
  return options.find((option) => option.groupBindingId === value || option.value === value) ?? null;
}

function currentGroupOption(sourceItem: KeyPoolItem | null, options: StationGroupOption[]) {
  if (
    !sourceItem?.groupBindingId ||
    options.some((option) => option.groupBindingId === sourceItem.groupBindingId || option.value === sourceItem.groupBindingId)
  ) {
    return [];
  }
  return [
    {
      value: sourceItem.groupBindingId,
      label: `${sourceItem.groupName ?? "当前绑定"} · ${formatRate(sourceItem.rateMultiplier)} · 当前`,
    },
  ];
}

function groupNameForEditSelection(
  value: string,
  sourceItem: KeyPoolItem | null,
  options: StationGroupOption[],
  fallback: string,
) {
  if (!value || value === KEEP_GROUP_BINDING_VALUE) {
    return sourceItem?.groupName ?? fallback;
  }
  if (value === CLEAR_GROUP_BINDING_VALUE) {
    return "";
  }
  if (value === sourceItem?.groupBindingId) {
    return sourceItem.groupName ?? fallback;
  }
  return selectedGroupOption(options, value)?.groupName ?? fallback;
}

function groupOptionLabel(option: StationGroupOption) {
  return `${option.groupName} · ${formatRate(option.rateMultiplier)} · ${option.rateSource ?? "可用"}`;
}

function linesToList(value: string) {
  return Array.from(
    new Set(
      value
        .split(/\r?\n/)
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  );
}

function commaListToList(value: string) {
  return Array.from(
    new Set(
      value
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  );
}

