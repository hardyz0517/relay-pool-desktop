import { useEffect, useRef, useState, type FormEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, Check, KeyRound, Loader2, Plus, RefreshCw, X } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, ConfirmDialog, EmptyState, IconButton, PageForm, SectionCard, SelectControl, SwitchControl, useToast } from "@/components/ui";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { getStationKeyCapabilities } from "@/lib/api/routing";
import { saveStationKeyWithDefaults } from "@/lib/api/stationKeys";
import { readError } from "@/lib/errors";
import { deriveStationGroupDisplayFacts } from "@/lib/projections/groupFacts";
import { queryKeys } from "@/lib/query/queryKeys";
import { keyPoolQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { cn } from "@/lib/utils";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import { StationGroupOptionLabel, StationGroupTriggerLabel } from "@/components/group/StationGroupChip";
import {
  buildStationGroupOptionsFromCurrentFactsForSelect,
  findMatchingGroupOption,
} from "@/lib/groupOptionViewModels";
import { OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS } from "./stationKeyCapabilityDefaults";
import { runStationKeyModelDiscoveryOperation } from "./modelDiscoveryOperationController";
import {
  addModelToList,
  applyDiscoveredModels,
  defaultModelFromPreferred,
  modelLines,
  preferredModelsFromDefault,
  removeModelFromList,
} from "./keyModelConfiguration";

type EditKeyPageProps = {
  stationKeyId: string | null;
  onBack: () => void;
  onUpdated?: () => void;
};

type EditKeyFormState = {
  id: string;
  stationId: string;
  stationName: string;
  stationApiBaseUrl: string;
  name: string;
  apiKey: string;
  enabled: boolean;
  priority: string;
  groupBindingId: string;
  groupName: string;
  tierLabel: string;
  note: string;
  supportsChatCompletions: boolean;
  supportsResponses: boolean;
  supportsEmbeddings: boolean;
  supportsStream: boolean;
  supportsTools: boolean;
  supportsVision: boolean;
  supportsReasoning: boolean;
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
  stationApiBaseUrl: "",
  name: "",
  apiKey: "",
  enabled: true,
  priority: "0",
  groupBindingId: "",
  groupName: "",
  tierLabel: "",
  note: "",
  supportsChatCompletions: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsChatCompletions,
  supportsResponses: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsResponses,
  supportsEmbeddings: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsEmbeddings,
  supportsStream: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsStream,
  supportsTools: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsTools,
  supportsVision: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsVision,
  supportsReasoning: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsReasoning,
  modelAllowlist: "",
  modelBlocklist: "",
  preferredModels: "",
  onlyUseAsBackup: false,
  routingTags: "",
};

const inputClassName =
  "h-8 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30 disabled:bg-surface-subtle disabled:text-muted-foreground";

const KEEP_GROUP_BINDING_VALUE = "__keep__";
const CLEAR_GROUP_BINDING_VALUE = "__clear__";
const emptyKeyPoolItems: KeyPoolItem[] = [];

export function EditKeyPage({ stationKeyId, onBack, onUpdated }: EditKeyPageProps) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const keyPoolItemsQuery = useActivityQuery(keyPoolQueryOptions());
  const keyPoolItems = keyPoolItemsQuery.data ?? emptyKeyPoolItems;
  const [sourceItem, setSourceItem] = useState<KeyPoolItem | null>(null);
  const [groupOptions, setGroupOptions] = useState<StationGroupOption[]>([]);
  const [form, setForm] = useState<EditKeyFormState>(emptyForm);
  const [initialFormSnapshot, setInitialFormSnapshot] = useState(() => serializeEditKeyForm(emptyForm));
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [fetchingModels, setFetchingModels] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const loadedStationKeyIdRef = useRef<string | null>(null);
  const modelDiscoveryAbortRef = useRef<AbortController | null>(null);
  const hasUnsavedChanges = serializeEditKeyForm(form) !== initialFormSnapshot;

  const bindingOptions = [
    ...groupOptions
      .filter((option) => option.groupBindingId)
      .map((option) => ({
        value: option.groupBindingId ?? option.value,
        label: groupOptionLabel(option),
        triggerLabel: groupTriggerLabel(option),
      })),
    ...currentGroupOption(sourceItem, groupOptions),
  ];

  useEffect(() => {
    if (stationKeyId && loadedStationKeyIdRef.current === stationKeyId) {
      return undefined;
    }
    let alive = true;
    setLoading(true);
    setError(null);
    setSourceItem(null);
    setGroupOptions([]);
    setForm(emptyForm);
    setInitialFormSnapshot(serializeEditKeyForm(emptyForm));

    if (!stationKeyId) {
      loadedStationKeyIdRef.current = null;
      setLoading(false);
      setError("未选择要编辑的密钥。");
      return () => {
        alive = false;
      };
    }
    if (keyPoolItemsQuery.isLoading) {
      return () => {
        alive = false;
      };
    }
    if (keyPoolItemsQuery.error) {
      loadedStationKeyIdRef.current = null;
      setLoading(false);
      setError(readError(keyPoolItemsQuery.error));
      return () => {
        alive = false;
      };
    }

    void Promise.resolve()
      .then(async () => {
        if (!alive) {
          return;
        }
        const item = keyPoolItems.find((candidate) => candidate.id === stationKeyId) ?? null;
        if (!item) {
          throw new Error("未找到要编辑的密钥。");
        }
        const [nextGroupOptions, capabilities] = await Promise.all([
          loadCurrentStationGroupOptions(item.stationId),
          getStationKeyCapabilities(item.id),
        ]);
        if (!alive) {
          return;
        }
        setSourceItem(item);
        setGroupOptions(nextGroupOptions);
        const nextForm = mergeCapabilitiesIntoForm(formFromItem(item, nextGroupOptions), capabilities);
        setForm(nextForm);
        setInitialFormSnapshot(serializeEditKeyForm(nextForm));
        loadedStationKeyIdRef.current = stationKeyId;
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
  }, [keyPoolItems, keyPoolItemsQuery.error, keyPoolItemsQuery.isLoading, stationKeyId, toast]);

  useEffect(() => () => modelDiscoveryAbortRef.current?.abort(), []);

  async function handleFetchModels() {
    if (!sourceItem || fetchingModels) {
      return;
    }
    if (form.apiKey.trim()) {
      toast.info("请先保存密钥", "获取模型列表会使用当前已保存的密钥。");
      return;
    }

    const abortController = new AbortController();
    modelDiscoveryAbortRef.current?.abort();
    modelDiscoveryAbortRef.current = abortController;
    setFetchingModels(true);
    try {
      const result = await runStationKeyModelDiscoveryOperation(sourceItem.id, {
        signal: abortController.signal,
      });
      if (result.models.length === 0) {
        toast.info("未获取到模型", "接口返回了空模型列表。");
        return;
      }
      const update = applyDiscoveredModels(
        result.models,
        defaultModelFromPreferred(form.preferredModels),
      );
      setForm((current) => ({
        ...current,
        modelAllowlist: update.modelAllowlist,
        preferredModels: update.preferredModels,
      }));
      if (update.defaultModelRemoved) {
        toast.info("模型列表已更新", `已获取 ${result.models.length} 个模型，原默认模型已不可用，请重新选择。`);
      } else {
        toast.success("模型列表已更新", `已获取 ${result.models.length} 个模型。`);
      }
    } catch (requestError) {
      if (!abortController.signal.aborted) {
        toast.error("获取模型列表失败", readError(requestError));
      }
    } finally {
      if (modelDiscoveryAbortRef.current === abortController) {
        modelDiscoveryAbortRef.current = null;
        setFetchingModels(false);
      }
    }
  }

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
        note: form.note.trim() ? form.note.trim() : null,
        groupSelection: groupSelectionFromEditForm(form, sourceItem, groupOptions),
        capabilities: capabilitiesFromEditForm(form),
      });
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: queryKeys.keyPool }),
        queryClient.invalidateQueries({ queryKey: queryKeys.stations }),
      ]);
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

  function requestExit() {
    if (hasUnsavedChanges) {
      setDiscardConfirmOpen(true);
      return;
    }
    onBack();
  }

  return (
    <PageScaffold
      title="编辑密钥"
      stickyHeader
      backAction={
        <IconButton label="返回密钥池" onClick={requestExit}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
      status={
        sourceItem ? (
          <span className="inline-flex h-6 items-center gap-1 rounded-[var(--surface-radius)] border border-border bg-surface px-2 text-xs font-medium text-muted-foreground">
            <KeyRound className="h-3.5 w-3.5" />
            {sourceItem.apiKeyMasked}
          </span>
        ) : undefined
      }
    >
      {loading ? (
        <div className="rounded-[var(--surface-radius)] border border-border bg-surface px-4 py-5 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
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
              <Button variant="secondary" onClick={requestExit} disabled={saving}>
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
                  <Field label="API Base URL">
                    <input className={inputClassName} value={form.stationApiBaseUrl} disabled />
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
                  <div className="mt-3 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
                    {error}
                  </div>
                )}
              </SectionCard>

            </div>

            <aside className="grid content-start gap-[var(--shell-page-gap)]">
              <SectionCard title="可选项">
                <div className="grid gap-3">
                  <Field label="分组">
                    <SelectControl
                      ariaLabel="分组"
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
                  <Field label="优先级">
                    <input className={inputClassName} type="number" value={form.priority} onChange={(event) => setForm({ ...form, priority: event.target.value })} />
                  </Field>
                  <Field label="档位">
                    <input className={inputClassName} value={form.tierLabel} onChange={(event) => setForm({ ...form, tierLabel: event.target.value })} />
                  </Field>
                  <Field label="启用状态">
                    <SwitchControl
                      ariaLabel="启用密钥"
                      checked={form.enabled}
                      className="justify-self-start"
                      offLabel="停用"
                      onCheckedChange={() => setForm({ ...form, enabled: !form.enabled })}
                      onLabel="启用"
                    />
                  </Field>
                  <Field label="路由标签">
                    <input className={inputClassName} value={form.routingTags} onChange={(event) => setForm({ ...form, routingTags: event.target.value })} placeholder="逗号分隔，例如：高优先级, 低延迟" />
                  </Field>
                  <Field label="备注">
                    <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.note} onChange={(event) => setForm({ ...form, note: event.target.value })} />
                  </Field>
                </div>
              </SectionCard>

              <SectionCard title="模型配置">
                <KeyModelConfigurationEditor
                  defaultModel={defaultModelFromPreferred(form.preferredModels)}
                  modelList={form.modelAllowlist}
                  modelListAction={
                    <Button
                      size="sm"
                      type="button"
                      variant="outline"
                      disabled={fetchingModels || saving}
                      title={form.apiKey.trim() ? "请先保存新密钥" : "使用当前密钥获取模型列表"}
                      onClick={() => void handleFetchModels()}
                    >
                      {fetchingModels ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <RefreshCw className="h-3.5 w-3.5" />
                      )}
                      {fetchingModels ? "获取中" : "获取模型"}
                    </Button>
                  }
                  onDefaultModelChange={(defaultModel) =>
                    setForm({ ...form, preferredModels: preferredModelsFromDefault(defaultModel) })
                  }
                  onModelListChange={(modelAllowlist) => setForm({ ...form, modelAllowlist })}
                />
              </SectionCard>
            </aside>
          </section>
        </PageForm>
      )}
      <ConfirmDialog
        open={discardConfirmOpen}
        title="放弃未保存修改？"
        description="当前密钥修改还没有保存，退出后这些修改会丢失。"
        confirmLabel="放弃修改"
        cancelLabel="继续编辑"
        onCancel={() => setDiscardConfirmOpen(false)}
        onConfirm={() => {
          setDiscardConfirmOpen(false);
          onBack();
        }}
      />
    </PageScaffold>
  );
}

export function KeyModelConfigurationEditor({
  defaultModel,
  modelList,
  modelListAction,
  onDefaultModelChange,
  onModelListChange,
}: {
  defaultModel: string;
  modelList: string;
  modelListAction: React.ReactNode;
  onDefaultModelChange: (model: string) => void;
  onModelListChange: (models: string) => void;
}) {
  const [newModel, setNewModel] = useState("");
  const models = modelLines(modelList);

  function addModel() {
    const model = newModel.trim();
    if (!model) {
      return;
    }
    onModelListChange(addModelToList(modelList, model));
    setNewModel("");
  }

  function removeModel(model: string) {
    onModelListChange(removeModelFromList(modelList, model));
    if (defaultModel.trim().toLowerCase() === model.toLowerCase()) {
      onDefaultModelChange("");
    }
  }

  return (
    <div className="grid gap-4">
      <div className="grid gap-1.5 text-xs font-medium text-muted-foreground">
        <div>默认模型</div>
        <div className="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] gap-2">
          <input
            aria-label="默认模型"
            className={`${inputClassName} min-w-0 w-full`}
            placeholder="输入模型名称或从列表选择"
            value={defaultModel}
            onChange={(event) => onDefaultModelChange(event.target.value)}
          />
          <SelectControl
            ariaLabel="从模型列表选择默认模型"
            className="h-8 w-8 min-w-[2rem] justify-center gap-0 px-0 shadow-none"
            disabled={models.length === 0}
            menuAlign="end"
            menuMinWidth={220}
            options={models.map((model) => ({ value: model, label: model }))}
            placeholder={null}
            value=""
            onChange={onDefaultModelChange}
          />
        </div>
      </div>

      <div className="grid gap-2">
        <div className="flex min-h-5 items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-2">
            <span className="text-xs font-medium text-muted-foreground">模型列表</span>
            <span className="text-xs tabular-nums text-muted-foreground">{models.length}</span>
          </div>
          {modelListAction}
        </div>
        <div className="flex min-w-0 items-center gap-1.5">
          <input
            className={`${inputClassName} min-w-0 flex-1`}
            placeholder="添加模型"
            value={newModel}
            onChange={(event) => setNewModel(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                addModel();
              }
            }}
          />
          <IconButton
            className="shrink-0 shadow-none"
            label="添加模型"
            disabled={!newModel.trim()}
            variant="outline"
            onClick={addModel}
          >
            <Plus className="h-4 w-4" />
          </IconButton>
        </div>
        {models.length === 0 ? (
          <div className="flex min-h-12 items-center justify-center border-y border-dashed border-border py-3 text-sm text-muted-foreground">
            暂无模型
          </div>
        ) : (
          <div aria-label="模型列表" className="flex flex-wrap gap-1.5" role="list">
            {models.map((model) => {
              const isDefault = defaultModel.trim().toLowerCase() === model.toLowerCase();
              return (
                <div
                  key={model}
                  className={cn(
                    "group flex h-8 max-w-full items-center gap-1.5 rounded-[6px] border border-border bg-surface-subtle pl-2.5 pr-1 text-foreground transition-colors hover:border-ring/30 hover:bg-hover",
                    isDefault && "border-info-border bg-info-surface text-info-foreground",
                  )}
                  role="listitem"
                >
                  <span className="min-w-0 max-w-64 truncate font-mono text-xs" title={model}>
                    {model}
                  </span>
                  {isDefault && (
                    <span className="shrink-0 text-[11px] font-medium text-info-foreground">
                      默认
                    </span>
                  )}
                  <IconButton
                    className="h-6 w-6 rounded-[5px] text-muted-foreground hover:bg-danger-surface hover:text-danger-foreground"
                    label={`移除模型 ${model}`}
                    variant="ghost"
                    onClick={() => removeModel(model)}
                  >
                    <X className="h-3.5 w-3.5" />
                  </IconButton>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
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
    <label className="flex items-center gap-2 text-sm text-foreground">
      <input
        checked={checked}
        className="h-4 w-4 accent-primary"
        type="checkbox"
        onChange={(event) => onChange(event.target.checked)}
      />
      {label}
    </label>
  );
}

function formFromItem(item: KeyPoolItem, options: StationGroupOption[] = []): EditKeyFormState {
  return {
    id: item.id,
    stationId: item.stationId,
    stationName: item.stationName,
    stationApiBaseUrl: item.stationApiBaseUrl,
    name: item.name,
    apiKey: "",
    enabled: item.enabled,
    priority: String(item.priority),
    groupBindingId: groupBindingValueFromItem(item, options),
    groupName: item.groupName ?? "",
    tierLabel: item.tierLabel ?? "",
    note: item.note ?? "",
    supportsChatCompletions: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsChatCompletions,
    supportsResponses: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsResponses,
    supportsEmbeddings: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsEmbeddings,
    supportsStream: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsStream,
    supportsTools: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsTools,
    supportsVision: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsVision,
    supportsReasoning: OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS.supportsReasoning,
    modelAllowlist: "",
    modelBlocklist: "",
    preferredModels: "",
    onlyUseAsBackup: item.onlyUseAsBackup,
    routingTags: "",
  };
}

function serializeEditKeyForm(form: EditKeyFormState) {
  return JSON.stringify(form);
}

function groupBindingValueFromItem(item: KeyPoolItem, options: StationGroupOption[]) {
  const option = findMatchingGroupOption(
    {
      groupBindingId: item.groupBindingId,
      groupIdHash: item.groupIdHash,
      groupName: item.groupName ?? "",
    },
    options,
  );
  return option?.groupBindingId ?? item.groupBindingId ?? KEEP_GROUP_BINDING_VALUE;
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

function capabilitiesFromEditForm(form: EditKeyFormState) {
  return {
    stationKeyId: form.id,
    supportsChatCompletions: form.supportsChatCompletions,
    supportsResponses: form.supportsResponses,
    supportsEmbeddings: form.supportsEmbeddings,
    supportsStream: form.supportsStream,
    supportsTools: form.supportsTools,
    supportsVision: form.supportsVision,
    supportsReasoning: form.supportsReasoning,
    modelAllowlist: linesToList(form.modelAllowlist),
    modelBlocklist: linesToList(form.modelBlocklist),
    preferredModels: linesToList(form.preferredModels),
    onlyUseAsBackup: form.onlyUseAsBackup,
    routingTags: commaListToList(form.routingTags),
  };
}

function mergeCapabilitiesIntoForm(
  form: EditKeyFormState,
  capabilities: StationKeyCapabilities,
): EditKeyFormState {
  return {
    ...form,
    supportsChatCompletions: capabilities.supportsChatCompletions,
    supportsResponses: capabilities.supportsResponses,
    supportsEmbeddings: capabilities.supportsEmbeddings,
    supportsStream: capabilities.supportsStream,
    supportsTools: capabilities.supportsTools,
    supportsVision: capabilities.supportsVision,
    supportsReasoning: capabilities.supportsReasoning,
    modelAllowlist: capabilities.modelAllowlist.join("\n"),
    modelBlocklist: capabilities.modelBlocklist.join("\n"),
    preferredModels: capabilities.preferredModels.join("\n"),
    onlyUseAsBackup: capabilities.onlyUseAsBackup,
    routingTags: capabilities.routingTags.join(", "),
  };
}

function selectedGroupOption(options: StationGroupOption[], value: string) {
  return options.find((option) => option.groupBindingId === value || option.value === value) ?? null;
}

async function loadCurrentStationGroupOptions(stationId: string) {
  const [bindings, rates] = await Promise.all([
    listStationGroupBindings(stationId),
    listGroupRateRecords(stationId),
  ]);
  return buildStationGroupOptionsFromCurrentFactsForSelect(
    deriveStationGroupDisplayFacts({ bindings, rates }),
  );
}

function currentGroupOption(sourceItem: KeyPoolItem | null, options: StationGroupOption[]) {
  if (!sourceItem?.groupBindingId || findMatchingGroupOption(keyPoolItemGroupRow(sourceItem), options)) {
    return [];
  }
  return [
    {
      value: sourceItem.groupBindingId,
      label: <StationGroupOptionLabel option={keyPoolItemGroupOption(sourceItem)} suffix="当前" />,
      triggerLabel: <StationGroupTriggerLabel option={keyPoolItemGroupOption(sourceItem)} suffix="当前" />,
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
  return <StationGroupOptionLabel option={option} />;
}

function groupTriggerLabel(option: StationGroupOption) {
  return <StationGroupTriggerLabel option={option} />;
}

function keyPoolItemGroupOption(item: KeyPoolItem) {
  return {
    groupName: item.groupName ?? "当前绑定",
    rateMultiplier: item.rateMultiplier,
  };
}

function keyPoolItemGroupRow(item: KeyPoolItem) {
  return {
    groupBindingId: item.groupBindingId,
    groupIdHash: item.groupIdHash,
    groupName: item.groupName ?? "",
  };
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
