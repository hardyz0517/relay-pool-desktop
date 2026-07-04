import { useEffect, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { Copy, Edit3, Plus, Trash2 } from "lucide-react";
import { Button, Dialog, StatusBadge, SwitchControl, useToast } from "@/components/ui";
import {
  createChannelMonitorTemplate,
  deleteChannelMonitorTemplate,
  duplicateChannelMonitorTemplate,
  updateChannelMonitorTemplate,
} from "@/lib/api/channelMonitors";
import type {
  ChannelMonitorRequestTemplate,
  CreateChannelMonitorTemplateInput,
} from "@/lib/types/channelMonitors";
import { cn } from "@/lib/utils";

type ChannelMonitorTemplateManagerProps = {
  open: boolean;
  templates: ChannelMonitorRequestTemplate[];
  onClose: () => void;
  onChanged: () => Promise<void> | void;
};

type TemplateDraft = {
  id: string | null;
  name: string;
  endpointKind: string;
  method: string;
  path: string;
  requestBodyJson: string;
  enabled: boolean;
  note: string;
};

type BusyState = {
  id: string;
  kind: "save" | "duplicate" | "delete" | "toggle";
} | null;

const allEndpointKindFilter = "__all__";

const inputClassName =
  "h-8 rounded-[8px] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:ring-2 focus:ring-teal-100";

const defaultRequestBodyJson = JSON.stringify(
  {
    model: "{{model}}",
    messages: [{ role: "user", content: "{{challenge}}" }],
    max_tokens: 1,
    stream: false,
  },
  null,
  2,
);

export function ChannelMonitorTemplateManager({
  open,
  templates,
  onClose,
  onChanged,
}: ChannelMonitorTemplateManagerProps) {
  const toast = useToast();
  const [draft, setDraft] = useState<TemplateDraft>(() => createEmptyDraft());
  const [endpointKindFilter, setEndpointKindFilter] = useState(allEndpointKindFilter);
  const [busyState, setBusyState] = useState<BusyState>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setDraft(createEmptyDraft());
      setError(null);
      setBusyState(null);
    }
  }, [open]);

  const endpointKindOptions = useMemo(() => {
    const counts = new Map<string, number>();
    for (const template of templates) {
      const key = template.endpointKind || "custom";
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    return [
      { value: allEndpointKindFilter, label: "全部", count: templates.length },
      ...[...counts.entries()]
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([value, count]) => ({
          value,
          label: formatEndpointKind(value),
          count,
        })),
    ];
  }, [templates]);

  useEffect(() => {
    if (
      endpointKindFilter !== allEndpointKindFilter &&
      !endpointKindOptions.some((option) => option.value === endpointKindFilter)
    ) {
      setEndpointKindFilter(allEndpointKindFilter);
    }
  }, [endpointKindFilter, endpointKindOptions]);

  const filteredTemplates = useMemo(
    () =>
      endpointKindFilter === allEndpointKindFilter
        ? templates
        : templates.filter((template) => (template.endpointKind || "custom") === endpointKindFilter),
    [endpointKindFilter, templates],
  );

  const groupedTemplates = useMemo(() => {
    const groups = new Map<string, ChannelMonitorRequestTemplate[]>();
    for (const template of filteredTemplates) {
      const key = template.endpointKind || "未分类";
      groups.set(key, [...(groups.get(key) ?? []), template]);
    }
    return [...groups.entries()].sort(([left], [right]) => left.localeCompare(right));
  }, [filteredTemplates]);

  const validationError = validateDraft(draft);
  const editingBuiltIn = templates.some((template) => template.id === draft.id && template.builtIn);
  const canSave = !validationError && !editingBuiltIn && busyState === null;
  const isSaving = busyState?.kind === "save";

  function updateDraft(patch: Partial<TemplateDraft>) {
    setDraft((current) => ({ ...current, ...patch }));
  }

  function editTemplate(template: ChannelMonitorRequestTemplate) {
    setError(null);
    setDraft({
      id: template.id,
      name: template.name,
      endpointKind: template.endpointKind,
      method: template.method,
      path: template.path,
      requestBodyJson: template.requestBodyJson,
      enabled: template.enabled,
      note: template.note ?? "",
    });
  }

  function createTemplate() {
    setError(null);
    setDraft(createEmptyDraft());
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canSave) {
      setError(validationError);
      return;
    }
    const input = draftToInput(draft);
    setBusyState({ id: draft.id ?? "new", kind: "save" });
    setError(null);
    try {
      if (draft.id) {
        await updateChannelMonitorTemplate({ ...input, id: draft.id });
        toast.success("模板已更新");
      } else {
        const created = await createChannelMonitorTemplate(input);
        toast.success("模板已创建");
        setDraft({
          id: created.id,
          name: created.name,
          endpointKind: created.endpointKind,
          method: created.method,
          path: created.path,
          requestBodyJson: created.requestBodyJson,
          enabled: created.enabled,
          note: created.note ?? "",
        });
      }
      await onChanged();
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("保存模板失败", message);
    } finally {
      setBusyState(null);
    }
  }

  async function handleDuplicate(template: ChannelMonitorRequestTemplate) {
    setBusyState({ id: template.id, kind: "duplicate" });
    setError(null);
    try {
      const copy = await duplicateChannelMonitorTemplate(template.id);
      toast.success("模板已复制");
      setDraft({
        id: copy.id,
        name: copy.name,
        endpointKind: copy.endpointKind,
        method: copy.method,
        path: copy.path,
        requestBodyJson: copy.requestBodyJson,
        enabled: copy.enabled,
        note: copy.note ?? "",
      });
      await onChanged();
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("复制模板失败", message);
    } finally {
      setBusyState(null);
    }
  }

  async function handleToggle(template: ChannelMonitorRequestTemplate) {
    if (template.builtIn) {
      return;
    }
    setBusyState({ id: template.id, kind: "toggle" });
    setError(null);
    try {
      await updateChannelMonitorTemplate({
        id: template.id,
        name: template.name,
        endpointKind: template.endpointKind,
        method: template.method.toUpperCase(),
        path: template.path,
        requestBodyJson: template.requestBodyJson,
        enabled: !template.enabled,
        note: template.note,
      });
      toast.success(template.enabled ? "模板已停用" : "模板已启用");
      if (draft.id === template.id) {
        updateDraft({ enabled: !template.enabled });
      }
      await onChanged();
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("更新模板状态失败", message);
    } finally {
      setBusyState(null);
    }
  }

  async function handleDelete(template: ChannelMonitorRequestTemplate) {
    if (template.builtIn || !window.confirm(`确认删除模板「${template.name}」？`)) {
      return;
    }
    setBusyState({ id: template.id, kind: "delete" });
    setError(null);
    try {
      await deleteChannelMonitorTemplate(template.id);
      toast.success("模板已删除");
      if (draft.id === template.id) {
        createTemplate();
      }
      await onChanged();
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("删除模板失败", message);
    } finally {
      setBusyState(null);
    }
  }

  return (
    <Dialog
      open={open}
      title="监控请求模板"
      description="管理本地探测请求"
      onClose={onClose}
      className="max-w-[1040px]"
      footer={
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0 truncate text-xs text-rose-600">{error ?? validationError ?? ""}</div>
          <div className="flex shrink-0 justify-end gap-2">
            <Button variant="outline" onClick={onClose} disabled={busyState !== null}>
              关闭
            </Button>
            <Button type="submit" form="channel-monitor-template-form" disabled={!canSave}>
              {isSaving ? "保存中" : "保存模板"}
            </Button>
          </div>
        </div>
      }
    >
      <div className="grid gap-0 md:grid-cols-[minmax(0,0.95fr)_minmax(360px,1.05fr)]">
        <div className="border-b border-border md:border-b-0 md:border-r">
          <div className="flex items-center justify-between gap-2 border-b border-border px-4 py-3">
            <div className="text-sm font-semibold text-slate-800">模板列表</div>
            <Button size="sm" variant="outline" onClick={createTemplate} disabled={busyState !== null}>
              <Plus className="h-3.5 w-3.5" />
              新建
            </Button>
          </div>
          <div className="flex flex-wrap gap-1.5 border-b border-border bg-slate-50/70 px-3 py-2">
            {endpointKindOptions.map((option) => {
              const selected = option.value === endpointKindFilter;
              return (
                <button
                  key={option.value}
                  type="button"
                  className={cn(
                    "inline-flex h-7 items-center gap-1 rounded-[7px] border px-2 text-xs font-medium transition-colors",
                    selected
                      ? "border-teal-200 bg-white text-teal-700 shadow-[var(--surface-shadow)]"
                      : "border-transparent text-slate-600 hover:border-border hover:bg-white",
                  )}
                  onClick={() => setEndpointKindFilter(option.value)}
                >
                  <span>{option.label}</span>
                  <span className="text-[11px] text-muted-foreground">{option.count}</span>
                </button>
              );
            })}
          </div>
          <div className="max-h-[520px] overflow-auto p-3">
            {templates.length === 0 ? (
              <div className="rounded-[8px] border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
                暂无模板
              </div>
            ) : filteredTemplates.length === 0 ? (
              <div className="rounded-[8px] border border-dashed border-border px-3 py-6 text-center text-sm text-muted-foreground">
                当前筛选下暂无模板
              </div>
            ) : (
              <div className="grid gap-3">
                {groupedTemplates.map(([endpointKind, group]) => (
                  <section key={endpointKind} className="grid gap-1.5">
                    <div className="px-1 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
                      {endpointKind}
                    </div>
                    <div className="grid gap-1.5">
                      {group.map((template) => {
                        const selected = template.id === draft.id;
                        const duplicating = busyState?.id === template.id && busyState.kind === "duplicate";
                        const deleting = busyState?.id === template.id && busyState.kind === "delete";
                        return (
                          <div
                            key={template.id}
                            className={cn(
                              "grid gap-2 rounded-[8px] border bg-white px-3 py-2 text-left shadow-[var(--surface-shadow)]",
                              selected ? "border-teal-300 ring-2 ring-teal-100" : "border-border",
                            )}
                          >
                            <button
                              type="button"
                              className="grid min-w-0 gap-1 text-left"
                              onClick={() => editTemplate(template)}
                            >
                              <div className="flex min-w-0 items-center gap-2">
                                <span className="truncate text-sm font-medium text-slate-800">{template.name}</span>
                                <StatusBadge tone={template.enabled ? "healthy" : "disabled"}>
                                  {template.enabled ? "启用" : "停用"}
                                </StatusBadge>
                                <StatusBadge tone={template.builtIn ? "info" : "warning"}>
                                  {template.builtIn ? "内置" : "自定义"}
                                </StatusBadge>
                              </div>
                              <div className="truncate text-[11px] text-muted-foreground">
                                {template.endpointKind} · {template.method.toUpperCase()} {template.path}
                              </div>
                              {template.note && (
                                <div className="line-clamp-2 text-[11px] text-slate-500">{template.note}</div>
                              )}
                            </button>
                            <div className="flex flex-wrap items-center justify-end gap-1.5">
                              {!template.builtIn && (
                                <Button
                                  size="sm"
                                  variant="ghost"
                                  disabled={busyState !== null}
                                  onClick={() => void handleToggle(template)}
                                >
                                  {template.enabled ? "停用" : "启用"}
                                </Button>
                              )}
                              <Button
                                size="sm"
                                variant="ghost"
                                disabled={busyState !== null}
                                onClick={() => editTemplate(template)}
                              >
                                <Edit3 className="h-3.5 w-3.5" />
                                {template.builtIn ? "查看" : "编辑"}
                              </Button>
                              <Button
                                size="sm"
                                variant="ghost"
                                disabled={busyState !== null}
                                onClick={() => void handleDuplicate(template)}
                              >
                                <Copy className="h-3.5 w-3.5" />
                                {duplicating ? "复制中" : "复制"}
                              </Button>
                              {!template.builtIn && (
                                <Button
                                  size="sm"
                                  variant="danger"
                                  disabled={busyState !== null}
                                  onClick={() => void handleDelete(template)}
                                >
                                  <Trash2 className="h-3.5 w-3.5" />
                                  {deleting ? "删除中" : "删除"}
                                </Button>
                              )}
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </section>
                ))}
              </div>
            )}
          </div>
        </div>

        <form id="channel-monitor-template-form" className="grid content-start gap-4 p-4" onSubmit={handleSubmit}>
          <div className="flex items-center justify-between gap-3">
            <div>
              <div className="text-sm font-semibold text-slate-800">{draft.id ? "编辑模板" : "新建模板"}</div>
              <div className="text-xs text-muted-foreground">
                {editingBuiltIn ? "内置模板只读，可复制后调整。" : "保存为自定义请求模板。"}
              </div>
            </div>
            <SwitchControl
              checked={draft.enabled}
              disabled={editingBuiltIn}
              ariaLabel="模板启用状态"
              onCheckedChange={() => updateDraft({ enabled: !draft.enabled })}
              onLabel="启用"
              offLabel="停用"
            />
          </div>

          <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_12rem]">
            <Field label="模板名称">
              <input
                className={inputClassName}
                value={draft.name}
                disabled={editingBuiltIn}
                onChange={(event) => updateDraft({ name: event.target.value })}
              />
            </Field>
            <Field label="端点类型">
              <input
                className={inputClassName}
                value={draft.endpointKind}
                disabled={editingBuiltIn}
                placeholder="chat_completions"
                onChange={(event) => updateDraft({ endpointKind: event.target.value })}
              />
            </Field>
          </div>

          <div className="grid gap-3 md:grid-cols-[8rem_minmax(0,1fr)]">
            <Field label="方法">
              <input
                className={inputClassName}
                value={draft.method}
                disabled={editingBuiltIn}
                onBlur={() => updateDraft({ method: draft.method.trim().toUpperCase() })}
                onChange={(event) => updateDraft({ method: event.target.value.toUpperCase() })}
              />
            </Field>
            <Field label="路径">
              <input
                className={inputClassName}
                value={draft.path}
                disabled={editingBuiltIn}
                placeholder="/v1/chat/completions"
                onChange={(event) => updateDraft({ path: event.target.value })}
              />
            </Field>
          </div>

          <Field label="请求体 JSON">
            <textarea
              className={`${inputClassName} min-h-[220px] resize-y py-2 font-mono text-xs leading-5`}
              value={draft.requestBodyJson}
              disabled={editingBuiltIn}
              spellCheck={false}
              onChange={(event) => updateDraft({ requestBodyJson: event.target.value })}
            />
          </Field>

          <Field label="备注">
            <textarea
              className={`${inputClassName} min-h-16 resize-none py-2`}
              value={draft.note}
              disabled={editingBuiltIn}
              onChange={(event) => updateDraft({ note: event.target.value })}
            />
          </Field>
        </form>
      </div>
    </Dialog>
  );
}

function createEmptyDraft(): TemplateDraft {
  return {
    id: null,
    name: "OpenAI 聊天探测",
    endpointKind: "chat_completions",
    method: "POST",
    path: "/v1/chat/completions",
    requestBodyJson: defaultRequestBodyJson,
    enabled: true,
    note: "",
  };
}

function validateDraft(draft: TemplateDraft) {
  if (!draft.name.trim()) {
    return "请填写模板名称";
  }
  if (!draft.endpointKind.trim()) {
    return "请填写端点类型";
  }
  if (!draft.method.trim()) {
    return "请填写请求方法";
  }
  if (!draft.path.trim()) {
    return "请填写请求路径";
  }
  try {
    JSON.parse(draft.requestBodyJson);
  } catch {
    return "请求体 JSON 格式不正确";
  }
  return null;
}

function draftToInput(draft: TemplateDraft): CreateChannelMonitorTemplateInput {
  return {
    name: draft.name.trim(),
    endpointKind: draft.endpointKind.trim(),
    method: draft.method.trim().toUpperCase(),
    path: draft.path.trim(),
    requestBodyJson: draft.requestBodyJson,
    enabled: draft.enabled,
    note: draft.note.trim() || null,
  };
}

function formatEndpointKind(endpointKind: string) {
  const labels: Record<string, string> = {
    chat_completions: "OpenAI Chat",
    responses: "Responses",
    models: "Models",
    embeddings: "Embeddings",
    custom: "自定义路径",
    custom_path: "自定义路径",
  };
  return labels[endpointKind] ?? endpointKind;
}

function Field({ label, children }: { label: string; children: ReactNode }) {
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
