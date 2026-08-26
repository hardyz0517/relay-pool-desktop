import { useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, FolderOpen, Globe2, Pencil, Plus, RefreshCw, RotateCcw, Search, SlidersHorizontal, Trash2 } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, Dialog, IconButton, SectionCard, SelectControl, SwitchControl, useToast } from "@/components/ui";
import { deleteModelBasePrice, getModelPriceSyncState, listModelPriceSyncCatalog, openModelPriceCatalogDirectory, reloadModelPriceCatalog, resetModelBasePricesToBuiltins, saveModelPriceSyncConfig, syncModelPrices, upsertModelBasePrice } from "@/lib/api/economics";
import { readError } from "@/lib/errors";
import { queryKeys } from "@/lib/query/queryKeys";
import { modelBasePricesQueryOptions, modelPriceSyncStateQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type { ModelBasePrice, ModelPriceCatalogEntry } from "@/lib/types/economics";
import type { RoutingDeepLink } from "@/lib/types/routingDeepLinks";

type PricingRoutingDeepLink = Extract<RoutingDeepLink, { kind: "simulate-model" }> & {
  source: "pricing";
};

type ModelBasePricesPageProps = {
  backLabel: string;
  onBack: () => void;
  onOpenRoutingDeepLink?: (link: PricingRoutingDeepLink) => void;
};

type DraftRow = {
  id?: string;
  provider: string;
  model: string;
  inputPrice: string;
  outputPrice: string;
  inputPricePriority: string;
  outputPricePriority: string;
  cacheCreationPrice: string;
  cacheCreationPricePriority: string;
  cacheCreationPriceAbove1Hr: string;
  cacheReadPrice: string;
  cacheReadPricePriority: string;
  longContextInputTokenThreshold: string;
  longContextInputCostMultiplier: string;
  longContextOutputCostMultiplier: string;
  supportsServiceTier: boolean;
  supportsPromptCaching: boolean;
  currency: string;
  unit: string;
  sourceUrl: string;
  sourceLabel: string;
  sourceCheckedAt: string;
  enabled: boolean;
  builtIn: boolean;
  note: string;
};

type ModelSelectionOption = {
  key: string;
  provider: string;
  model: string;
  name: string;
  releaseDate: string;
  common: boolean;
  inputPrice: number | null;
  outputPrice: number | null;
  cacheReadPrice: number | null;
  cacheCreationPrice: number | null;
};

type ModelPickerProvider = "all" | string;

const MODEL_PICKER_VISIBLE_LIMIT = 300;
const COMMON_MODEL_LIMIT_PER_FAMILY = 6;

const commonModelFamilyRules: Array<{
  providers: ReadonlySet<string>;
  matches: (model: string) => boolean;
}> = [
  { providers: new Set(["anthropic"]), matches: (model) => model.startsWith("claude-") },
  { providers: new Set(["openai"]), matches: (model) => ["gpt-", "o1-", "o3-", "o4-"].some((prefix) => model.startsWith(prefix)) },
  { providers: new Set(["google"]), matches: (model) => model.startsWith("gemini-") },
  { providers: new Set(["xai"]), matches: (model) => model.startsWith("grok-") },
  { providers: new Set(["deepseek"]), matches: (model) => model.startsWith("deepseek-") },
  { providers: new Set(["alibaba", "alibaba-cn"]), matches: (model) => model.startsWith("qwen") },
  { providers: new Set(["xiaomi"]), matches: (model) => model.startsWith("mimo-") },
  { providers: new Set(["longcat"]), matches: (model) => model.startsWith("longcat-") },
  { providers: new Set(["moonshotai", "moonshotai-cn", "kimi-for-coding"]), matches: (model) => model.startsWith("kimi-") },
  { providers: new Set(["minimax", "minimax-cn"]), matches: (model) => model.startsWith("minimax-") },
  { providers: new Set(["zai", "zhipuai"]), matches: (model) => model.startsWith("glm-") },
];

function createEmptyDraft(): DraftRow {
  return {
    provider: "custom",
    model: "",
    inputPrice: "0",
    outputPrice: "0",
    inputPricePriority: "",
    outputPricePriority: "",
    cacheCreationPrice: "0",
    cacheCreationPricePriority: "",
    cacheCreationPriceAbove1Hr: "",
    cacheReadPrice: "0",
    cacheReadPricePriority: "",
    longContextInputTokenThreshold: "",
    longContextInputCostMultiplier: "",
    longContextOutputCostMultiplier: "",
    supportsServiceTier: false,
    supportsPromptCaching: false,
    currency: "USD",
    unit: "M",
    sourceUrl: "",
    sourceLabel: "Manual",
    sourceCheckedAt: formatLocalDate(new Date()),
    enabled: true,
    builtIn: false,
    note: "",
  };
}

export function ModelBasePricesPage({
  backLabel,
  onBack,
}: ModelBasePricesPageProps) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const modelBasePricesQuery = useActivityQuery(modelBasePricesQueryOptions());
  const modelPriceSyncStateQuery = useActivityQuery(modelPriceSyncStateQueryOptions());
  const syncState = modelPriceSyncStateQuery.data;
  const rows = modelBasePricesQuery.data ?? [];
  const [query, setQuery] = useState("");
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [createDisplayName, setCreateDisplayName] = useState("");
  const [createImportOpen, setCreateImportOpen] = useState(false);
  const [editTarget, setEditTarget] = useState<ModelBasePrice | null>(null);
  const [editDraft, setEditDraft] = useState<DraftRow | null>(null);
  const [editDisplayName, setEditDisplayName] = useState("");
  const [modelPickerOpen, setModelPickerOpen] = useState(false);
  const [modelPriceCatalog, setModelPriceCatalog] = useState<ModelPriceCatalogEntry[] | null>(null);
  const [modelPriceCatalogLoading, setModelPriceCatalogLoading] = useState(false);
  const [modelPriceCatalogError, setModelPriceCatalogError] = useState<string | null>(null);
  const [createDraft, setCreateDraft] = useState<DraftRow>(() => createEmptyDraft());
  const [saving, setSaving] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [reloadingCatalog, setReloadingCatalog] = useState(false);
  const [openingCatalogDirectory, setOpeningCatalogDirectory] = useState(false);
  const [savingSyncConfig, setSavingSyncConfig] = useState(false);
  const [selectedModelKeysDraft, setSelectedModelKeysDraft] = useState<Set<string>>(() => new Set());
  const [includeCommonModelsDraft, setIncludeCommonModelsDraft] = useState(true);
  const [modelPickerQuery, setModelPickerQuery] = useState("");
  const [modelPickerProvider, setModelPickerProvider] = useState<ModelPickerProvider>("all");
  const autoSyncAttempted = useRef(false);
  const [savingKeys, setSavingKeys] = useState<Set<string>>(() => new Set());
  const [error, setError] = useState<string | null>(null);
  const loading = modelBasePricesQuery.isPending && modelBasePricesQuery.data === undefined;
  const displayError = error ?? (modelBasePricesQuery.error ? readError(modelBasePricesQuery.error) : null);

  useEffect(() => {
    if (!syncState?.autoSyncEnabled || autoSyncAttempted.current) return;
    autoSyncAttempted.current = true;
    void runModelPriceSync(false);
  }, [syncState?.autoSyncEnabled]);

  async function refresh(showSuccess = false) {
    setReloadingCatalog(true);
    setError(null);
    try {
      const nextState = await reloadModelPriceCatalog();
      setModelPriceCatalog(null);
      queryClient.setQueryData(queryKeys.modelPriceSyncState, nextState);
      await queryClient.invalidateQueries({ queryKey: queryKeys.modelBasePrices });
      await queryClient.invalidateQueries({ queryKey: queryKeys.modelPriceSyncState });
      if (showSuccess) {
        toast.success("模型基准价格已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("读取模型基准价格失败", message);
    } finally {
      setReloadingCatalog(false);
    }
  }

  async function runModelPriceSync(force: boolean) {
    setSyncing(true);
    setError(null);
    try {
      const result = await syncModelPrices(force);
      setModelPriceCatalog(null);
      queryClient.setQueryData(queryKeys.modelPriceSyncState, result.state);
      await queryClient.invalidateQueries({ queryKey: queryKeys.modelBasePrices });
      await queryClient.invalidateQueries({ queryKey: queryKeys.pricing });
      toast.success(
        force
          ? `已刷新 ${result.state.modelCount} 条供应商报价，并更新 ${result.importedCount} 个有效模型价格`
          : result.importedCount
            ? `已自动更新 ${result.importedCount} 个模型价格`
            : "没有需要自动同步的模型",
      );
    } catch (requestError) {
      let message = readError(requestError);
      try {
        const nextState = await getModelPriceSyncState();
        queryClient.setQueryData(queryKeys.modelPriceSyncState, nextState);
        message = nextState.lastSyncError ?? message;
      } catch {
        await queryClient.invalidateQueries({ queryKey: queryKeys.modelPriceSyncState });
      }
      setError(message);
      toast.error("同步模型价格失败", message);
    } finally {
      setSyncing(false);
    }
  }

  async function updateAutoSync(enabled: boolean) {
    setSavingSyncConfig(true);
    try {
      const next = await saveModelPriceSyncConfig({
        autoSyncEnabled: enabled,
        includeCommonModels: syncState?.includeCommonModels ?? true,
        selectedModelKeys: syncState?.selectedModelKeys ?? [],
        excludedCommonModelKeys: syncState?.excludedCommonModelKeys ?? [],
      });
      queryClient.setQueryData(queryKeys.modelPriceSyncState, next);
      toast.success(enabled ? "已开启自动同步" : "已关闭自动同步");
    } catch (requestError) {
      toast.error("保存同步设置失败", readError(requestError));
    } finally {
      setSavingSyncConfig(false);
    }
  }

  async function openCatalogDirectory() {
    setOpeningCatalogDirectory(true);
    try {
      await openModelPriceCatalogDirectory();
    } catch (requestError) {
      toast.error("打开本地定价目录失败", readError(requestError));
    } finally {
      setOpeningCatalogDirectory(false);
    }
  }

  async function loadModelPriceCatalog() {
    setModelPriceCatalogLoading(true);
    setModelPriceCatalogError(null);
    try {
      setModelPriceCatalog(await listModelPriceSyncCatalog());
    } catch (requestError) {
      setModelPriceCatalogError(readError(requestError));
    } finally {
      setModelPriceCatalogLoading(false);
    }
  }

  function openModelPicker() {
    if (!syncState) {
      return;
    }
    setSelectedModelKeysDraft(new Set(syncState.selectedModelKeys.map(normalizeModelKey)));
    setIncludeCommonModelsDraft(syncState.includeCommonModels);
    setModelPickerQuery("");
    setModelPickerProvider("all");
    setModelPickerOpen(true);
    if (modelPriceCatalog === null) {
      void loadModelPriceCatalog();
    }
  }

  function toggleModelSelection(option: ModelSelectionOption) {
    setSelectedModelKeysDraft((current) => {
      const next = new Set(current);
      if (next.has(option.key)) {
        next.delete(option.key);
      } else {
        next.add(option.key);
      }
      return next;
    });
  }

  function setCommonModelsSelected(selected: boolean) {
    setIncludeCommonModelsDraft(selected);
    setSelectedModelKeysDraft((current) => {
      const next = new Set(current);
      for (const option of modelSelectionOptions) {
        if (!option.common) {
          continue;
        }
        if (selected) {
          next.add(option.key);
        } else {
          next.delete(option.key);
        }
      }
      return next;
    });
  }

  function setFilteredModelsSelected(selected: boolean) {
    setSelectedModelKeysDraft((current) => {
      const next = new Set(current);
      for (const option of filteredModelSelectionOptions) {
        if (selected) {
          next.add(option.key);
        } else {
          next.delete(option.key);
        }
      }
      return next;
    });
  }

  async function saveModelSelection() {
    if (!syncState) {
      return;
    }
    setSavingSyncConfig(true);
    try {
      const next = await saveModelPriceSyncConfig({
        autoSyncEnabled: syncState.autoSyncEnabled,
        includeCommonModels: includeCommonModelsDraft,
        selectedModelKeys: [...selectedModelKeysDraft]
          .filter((key) => !includeCommonModelsDraft || !commonModelKeys.has(key))
          .sort(),
        excludedCommonModelKeys: includeCommonModelsDraft
          ? [...commonModelKeys].filter((key) => !selectedModelKeysDraft.has(key)).sort()
          : [],
      });
      queryClient.setQueryData(queryKeys.modelPriceSyncState, next);
      setModelPickerOpen(false);
      toast.success("模型选择已保存，将在下次同步时生效");
    } catch (requestError) {
      toast.error("保存模型选择失败", readError(requestError));
    } finally {
      setSavingSyncConfig(false);
    }
  }

  async function resetBuiltins() {
    setSaving(true);
    setError(null);
    try {
      const nextRows = await resetModelBasePricesToBuiltins();
      queryClient.setQueryData(queryKeys.modelBasePrices, nextRows);
      await queryClient.invalidateQueries({ queryKey: queryKeys.pricing });
      toast.success("已恢复内置基准价格");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("恢复内置价格失败", message);
    } finally {
      setSaving(false);
    }
  }

  async function saveCreateDraft() {
    if (!createDraft.model.trim() || !createDisplayName.trim()) {
      toast.error("请填写模型 ID 和显示名称");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const normalizedModel = normalizeCatalogModelIdForPricing(createDraft.model);
      if (!normalizedModel) {
        toast.error("模型 ID 格式无效");
        return;
      }
      const existing = rows.find((row) => normalizeCatalogModelIdForPricing(row.model) === normalizedModel);
      const saved = await upsertModelBasePrice(draftToInput({
        ...createDraft,
        id: existing?.id ?? stableModelPriceId(normalizedModel),
        model: normalizedModel,
        note: replaceModelPriceDisplayName(createDraft.note, createDisplayName),
      }));
      queryClient.setQueryData(queryKeys.modelBasePrices, (currentRows: ModelBasePrice[] | undefined) =>
        upsertRow(currentRows ?? [], saved),
      );
      await queryClient.invalidateQueries({ queryKey: queryKeys.pricing });
      setCreateDialogOpen(false);
      setCreateImportOpen(false);
      setCreateDraft(createEmptyDraft());
      setCreateDisplayName("");
      toast.success(existing ? "模型价格已更新" : "模型价格已添加");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("新增模型基准价格失败", message);
    } finally {
      setSaving(false);
    }
  }

  function openEditDialog(row: ModelBasePrice) {
    setCreateImportOpen(false);
    setEditTarget(row);
    setEditDraft(rowToDraft(row));
    setEditDisplayName(modelPriceDisplayName(row));
  }

  function closeEditDialog() {
    if (saving) {
      return;
    }
    setEditTarget(null);
    setEditDraft(null);
    setEditDisplayName("");
    setCreateImportOpen(false);
  }

  async function saveEditDraft() {
    if (!editTarget || !editDraft || !editDraft.model.trim() || !editDisplayName.trim()) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const normalizedModel = normalizeCatalogModelIdForPricing(editDraft.model);
      if (!normalizedModel) {
        toast.error("模型 ID 格式无效");
        return;
      }
      const saved = await upsertModelBasePrice(draftToInput({
        ...editDraft,
        model: normalizedModel,
        note: replaceModelPriceDisplayName(editDraft.note, editDisplayName),
      }));
      queryClient.setQueryData(queryKeys.modelBasePrices, (currentRows: ModelBasePrice[] | undefined) =>
        upsertRow(currentRows ?? rows, saved),
      );
      await queryClient.invalidateQueries({ queryKey: queryKeys.pricing });
      setEditTarget(null);
      setEditDraft(null);
      setEditDisplayName("");
      toast.success("模型基准价格已保存");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("保存模型基准价格失败", message);
    } finally {
      setSaving(false);
    }
  }

  async function deleteRow(row: ModelBasePrice) {
    if (!window.confirm(`确定删除模型“${row.model}”的有效价格吗？`)) {
      return;
    }
    const savingKey = `${row.id}:delete`;
    setSavingKeys((current) => new Set(current).add(savingKey));
    setError(null);
    try {
      await deleteModelBasePrice(row.id);
      queryClient.setQueryData(queryKeys.modelBasePrices, (currentRows: ModelBasePrice[] | undefined) =>
        (currentRows ?? rows).filter((item) => item.id !== row.id),
      );
      await queryClient.invalidateQueries({ queryKey: queryKeys.pricing });
      toast.success("模型基准价格已删除");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("删除模型基准价格失败", message);
    } finally {
      setSavingKeys((current) => {
        const next = new Set(current);
        next.delete(savingKey);
        return next;
      });
    }
  }

  const metrics = useMemo(() => {
    const enabled = rows.filter((row) => row.enabled).length;
    const builtIn = rows.filter((row) => row.builtIn).length;
    return { enabled, builtIn, total: rows.length };
  }, [rows]);

  const visibleRows = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    return rows
      .filter((row) => !normalizedQuery || [row.model, modelPriceDisplayName(row), row.note ?? ""].some((value) =>
        value.toLowerCase().includes(normalizedQuery),
      ))
      .sort(compareRows);
  }, [query, rows]);

  const modelSelectionOptions = useMemo(() => {
    const options = new Map<string, ModelSelectionOption>();
    for (const entry of modelPriceCatalog ?? []) {
      const key = normalizeModelKey(entry.key);
      options.set(key, {
        key,
        provider: entry.provider,
        model: entry.model,
        name: entry.name,
        releaseDate: entry.releaseDate ?? "",
        common: false,
        inputPrice: entry.inputPrice,
        outputPrice: entry.outputPrice,
        cacheReadPrice: entry.cacheReadPrice,
        cacheCreationPrice: entry.cacheCreationPrice,
      });
    }
    for (const key of syncState?.selectedModelKeys ?? []) {
      addMissingModelSelectionOption(options, key, false);
    }
    const sorted = [...options.values()].sort((left, right) =>
      right.releaseDate.localeCompare(left.releaseDate)
        || left.name.localeCompare(right.name)
        || left.key.localeCompare(right.key),
    );
    const commonKeys = getCommonModelKeys(sorted);
    return sorted.map((option) => ({ ...option, common: commonKeys.has(option.key) }));
  }, [modelPriceCatalog, syncState?.selectedModelKeys]);

  const modelPickerProviderOptions = useMemo(() => {
    const providers = [...new Set(modelSelectionOptions.map((option) => option.provider))].sort((left, right) =>
      left.localeCompare(right),
    );
    return [
      { value: "all", label: "全部供应商" },
      ...providers.map((provider) => ({ value: provider, label: provider })),
    ];
  }, [modelSelectionOptions]);

  const filteredModelSelectionOptions = useMemo(() => {
    const normalizedQuery = modelPickerQuery.trim().toLowerCase();
    return modelSelectionOptions.filter((option) => {
      if (modelPickerProvider !== "all" && option.provider !== modelPickerProvider) {
        return false;
      }
      return !normalizedQuery || [option.name, option.model, option.provider, option.key].some((value) =>
        value.toLowerCase().includes(normalizedQuery),
      );
    });
  }, [modelPickerProvider, modelPickerQuery, modelSelectionOptions]);

  const visibleModelSelectionOptions = filteredModelSelectionOptions.slice(0, MODEL_PICKER_VISIBLE_LIMIT);
  const commonModelKeys = useMemo(
    () => new Set(modelSelectionOptions.filter((option) => option.common).map((option) => option.key)),
    [modelSelectionOptions],
  );
  const commonModelSelectionCount = modelSelectionOptions.filter((option) => option.common).length;
  const allCommonModelsSelected = commonModelSelectionCount > 0 && modelSelectionOptions
    .filter((option) => option.common)
    .every((option) => selectedModelKeysDraft.has(option.key));

  useEffect(() => {
    if (!modelPickerOpen || !includeCommonModelsDraft || commonModelKeys.size === 0) {
      return;
    }
    const excluded = new Set((syncState?.excludedCommonModelKeys ?? []).map(normalizeModelKey));
    setSelectedModelKeysDraft((current) => {
      const next = new Set(current);
      for (const key of commonModelKeys) {
        if (!excluded.has(key)) {
          next.add(key);
        }
      }
      return next;
    });
  }, [commonModelKeys, includeCommonModelsDraft, modelPickerOpen, syncState?.excludedCommonModelKeys]);

  function openCreateDialog() {
    setCreateDraft(createEmptyDraft());
    setCreateDisplayName("");
    setCreateImportOpen(false);
    setCreateDialogOpen(true);
  }

  function closeCreatePage() {
    if (saving) {
      return;
    }
    setCreateDialogOpen(false);
    setCreateImportOpen(false);
  }

  function openCatalogImport() {
    setModelPickerQuery("");
    setModelPickerProvider("all");
    setCreateImportOpen(true);
    if (modelPriceCatalog === null) {
      void loadModelPriceCatalog();
    }
  }

  function applyImportedCatalogModel(option: ModelSelectionOption) {
    const model = normalizeCatalogModelIdForPricing(option.model);
    const existing = rows.find((row) => normalizeCatalogModelIdForPricing(row.model) === model);
    const noteSuffix = option.releaseDate ? `; released ${option.releaseDate}; USD per M tokens` : "; USD per M tokens";
    const importedDraft = {
      ...(editDraft ?? createEmptyDraft()),
      id: editTarget?.id ?? existing?.id ?? stableModelPriceId(model),
      provider: option.provider,
      model,
      inputPrice: formatImportedPrice(option.inputPrice),
      outputPrice: formatImportedPrice(option.outputPrice),
      cacheReadPrice: formatImportedPrice(option.cacheReadPrice),
      cacheCreationPrice: formatImportedPrice(option.cacheCreationPrice),
      supportsPromptCaching: option.cacheReadPrice !== null || option.cacheCreationPrice !== null,
      sourceUrl: syncState?.sourceUrl ?? "https://models.dev/api.json",
      sourceLabel: "models.dev",
      note: `${option.name}${noteSuffix}`,
    };
    if (editTarget && editDraft) {
      setEditDraft(importedDraft);
      setEditDisplayName(option.name);
    } else {
      setCreateDraft(importedDraft);
      setCreateDisplayName(option.name);
    }
    setCreateImportOpen(false);
  }

  function updateEditorDraft(next: DraftRow) {
    if (createDialogOpen) {
      setCreateDraft(next);
    } else {
      setEditDraft(next);
    }
  }

  function updateEditorDisplayName(next: string) {
    if (createDialogOpen) {
      setCreateDisplayName(next);
    } else {
      setEditDisplayName(next);
    }
  }

  function closeEditorPage() {
    if (createDialogOpen) {
      closeCreatePage();
    } else {
      closeEditDialog();
    }
  }

  const editorMode = createDialogOpen ? "create" : editTarget && editDraft ? "edit" : null;
  const editorDraft = editorMode === "create" ? createDraft : editorMode === "edit" ? editDraft : null;
  const editorDisplayName = editorMode === "create" ? createDisplayName : editDisplayName;

  if (editorMode && editorDraft) {
    const creating = editorMode === "create";
    return (
      <PageScaffold
        fill
        title={creating ? "新增定价" : "编辑定价"}
        stickyHeader
        backAction={
          <IconButton label="返回模型基准价格" disabled={saving} onClick={closeEditorPage}>
            <ArrowLeft className="h-4 w-4" />
          </IconButton>
        }
      >
        <div className="flex min-h-0 flex-1 flex-col gap-5 pt-1">
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2.5 text-xs text-muted-foreground">
            <span>
              {creating
                ? "无需手动填写，可从 models.dev 选择模型定价"
                : "可从 models.dev 重新选择模型并覆盖当前定价"}
            </span>
            <Button variant="outline" onClick={openCatalogImport}>
              <Globe2 className="h-4 w-4" />
              从 models.dev 导入
            </Button>
          </div>

          <div className="grid content-start gap-5">
            <Field
              label="模型 ID"
              placeholder="例如: claude-3-5-sonnet-20241022"
              value={editorDraft.model}
              onChange={(model) => updateEditorDraft({ ...editorDraft, model })}
            />
            <Field
              label="显示名称"
              placeholder="例如: Claude 3.5 Sonnet"
              value={editorDisplayName}
              onChange={updateEditorDisplayName}
            />
            <Field
              label="输入成本（每百万 tokens, USD）"
              numeric
              value={editorDraft.inputPrice}
              onChange={(inputPrice) => updateEditorDraft({ ...editorDraft, inputPrice })}
            />
            <Field
              label="输出成本（每百万 tokens, USD）"
              numeric
              value={editorDraft.outputPrice}
              onChange={(outputPrice) => updateEditorDraft({ ...editorDraft, outputPrice })}
            />
            <Field
              label="缓存读取成本（每百万 tokens, USD）"
              numeric
              value={editorDraft.cacheReadPrice}
              onChange={(cacheReadPrice) => updateEditorDraft({ ...editorDraft, cacheReadPrice })}
            />
            <Field
              label="缓存写入成本（每百万 tokens, USD）"
              numeric
              value={editorDraft.cacheCreationPrice}
              onChange={(cacheCreationPrice) => updateEditorDraft({ ...editorDraft, cacheCreationPrice })}
            />
          </div>

          {displayError ? <div className="text-xs text-danger-foreground">{displayError}</div> : null}

          <div className="sticky bottom-0 mt-auto flex justify-end border-t border-border bg-background py-3">
            <Button
              disabled={saving || !editorDraft.model.trim() || !editorDisplayName.trim()}
              onClick={() => void (creating ? saveCreateDraft() : saveEditDraft())}
            >
              {creating ? <Plus className="h-4 w-4" /> : null}
              {saving ? (creating ? "添加中" : "保存中") : creating ? "添加" : "保存"}
            </Button>
          </div>
        </div>

        <Dialog
          open={createImportOpen}
          title="从 models.dev 导入定价"
          description={`搜索并选择一个模型，定价会自动填入${creating ? "新增" : "编辑"}表单。`}
          className="max-w-[920px]"
          footer={
            <div className="flex justify-end">
              <Button variant="outline" onClick={() => setCreateImportOpen(false)}>
                取消
              </Button>
            </div>
          }
          onClose={() => setCreateImportOpen(false)}
        >
          {modelPriceCatalogLoading ? (
            <div className="flex min-h-[260px] items-center justify-center gap-2 px-5 py-10 text-sm text-muted-foreground">
              <RefreshCw className="h-4 w-4 animate-spin" />
              正在读取完整模型目录…
            </div>
          ) : modelPriceCatalogError ? (
            <div className="grid min-h-[260px] place-content-center justify-items-center gap-3 px-5 py-10 text-center">
              <div className="text-sm text-danger-foreground">读取完整模型目录失败：{modelPriceCatalogError}</div>
              <Button variant="outline" onClick={() => void loadModelPriceCatalog()}>
                <RefreshCw className="h-4 w-4" />
                重试
              </Button>
            </div>
          ) : modelSelectionOptions.length === 0 ? (
            <div className="px-5 py-10 text-center text-sm text-muted-foreground">
              暂无可导入模型，请先返回价格清单并点击“立即同步”获取完整模型目录。
            </div>
          ) : (
            <div className="grid gap-3 p-5">
              <div className="flex flex-wrap items-center gap-2">
                <SelectControl
                  ariaLabel="筛选导入模型供应商"
                  className="min-w-[180px]"
                  options={modelPickerProviderOptions}
                  value={modelPickerProvider}
                  onChange={setModelPickerProvider}
                />
                <div className="relative min-w-[240px] flex-1">
                  <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/70" />
                  <input
                    aria-label="搜索可导入模型"
                    className="h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface pl-8 pr-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30"
                    placeholder="搜索模型或供应商（全量搜索）..."
                    value={modelPickerQuery}
                    onChange={(event) => setModelPickerQuery(event.target.value)}
                  />
                </div>
              </div>

              <div className="max-h-[calc(100vh-330px)] min-h-[260px] overflow-y-auto rounded-[var(--surface-radius)] border border-border">
                {filteredModelSelectionOptions.length === 0 ? (
                  <div className="flex min-h-[260px] items-center justify-center px-4 text-sm text-muted-foreground">
                    没有匹配的模型
                  </div>
                ) : (
                  <div className="divide-y divide-border">
                    {visibleModelSelectionOptions.map((option) => (
                      <button
                        key={option.key}
                        type="button"
                        className="flex w-full min-w-0 cursor-pointer items-center gap-3 px-3 py-2 text-left transition hover:bg-hover focus-visible:bg-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/30"
                        onClick={() => applyImportedCatalogModel(option)}
                      >
                        <span className="min-w-0 flex-1">
                          <span className="flex min-w-0 items-center gap-2">
                            <span className="truncate text-sm font-medium text-foreground">{option.name}</span>
                            <span className="shrink-0 text-xs text-muted-foreground">{option.provider}</span>
                            {option.releaseDate ? (
                              <span className="shrink-0 text-[10px] text-muted-foreground/75">{option.releaseDate}</span>
                            ) : null}
                          </span>
                          <span className="mt-0.5 block truncate font-mono text-xs text-muted-foreground" title={option.model}>
                            {option.model}
                          </span>
                        </span>
                        <span className="hidden shrink-0 grid-cols-4 gap-3 text-right sm:grid">
                          <ModelPickerPrice label="输入成本" value={option.inputPrice} />
                          <ModelPickerPrice label="输出成本" value={option.outputPrice} />
                          <ModelPickerPrice label="缓存读取" value={option.cacheReadPrice} />
                          <ModelPickerPrice label="缓存写入" value={option.cacheCreationPrice} />
                        </span>
                      </button>
                    ))}
                    {filteredModelSelectionOptions.length > visibleModelSelectionOptions.length ? (
                      <div className="px-3 py-2 text-center text-xs text-muted-foreground">
                        当前显示前 {visibleModelSelectionOptions.length} 条，共 {filteredModelSelectionOptions.length} 条；请使用搜索缩小范围。
                      </div>
                    ) : null}
                  </div>
                )}
              </div>
            </div>
          )}
        </Dialog>
      </PageScaffold>
    );
  }

  return (
    <PageScaffold
      title="模型基准价格"
      stickyHeader
      backAction={
        <IconButton label={backLabel} onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
      actions={
        <>
          <Button variant="outline" onClick={openCreateDialog}>
            <Plus className="h-4 w-4" />
            新增
          </Button>
          <Button disabled={loading || saving || reloadingCatalog} variant="outline" onClick={() => void refresh(true)}>
            <RefreshCw className="h-4 w-4" />
            {reloadingCatalog ? "刷新中" : "刷新"}
          </Button>
          <Button disabled={saving} variant="outline" onClick={() => void resetBuiltins()}>
            <RotateCcw className="h-4 w-4" />
            恢复内置
          </Button>
        </>
      }
    >
      <div className="grid min-w-0 gap-[var(--shell-page-gap)]">
        <section className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-surface">
          <div className="grid gap-5 px-5 py-4">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div className="min-w-0 flex-1">
                <h2 className="text-sm font-semibold text-foreground">自动同步 models.dev 定价</h2>
                <p className="mt-1 max-w-[820px] text-xs leading-5 text-muted-foreground">
                  开启后会定期更新勾选模型的价格；“立即同步”始终全量更新 models.dev 价格目录。
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-3 text-xs text-muted-foreground">
                <span>{syncState?.autoSyncEnabled ? "已开启" : "未开启"}</span>
                <SwitchControl
                  ariaLabel="自动同步模型价格"
                  checked={syncState?.autoSyncEnabled ?? false}
                  disabled={!syncState || syncing || savingSyncConfig}
                  showLabel={false}
                  onCheckedChange={() => void updateAutoSync(!(syncState?.autoSyncEnabled ?? false))}
                />
              </div>
            </div>

            <div className="grid gap-x-8 gap-y-2 text-xs text-muted-foreground sm:grid-cols-2">
              <div>上次同步：{syncState?.lastSyncAt ? formatSyncTime(syncState.lastSyncAt) : "尚未同步"}</div>
              <div>自动同步范围：{syncState ? `${syncState.autoSyncModelCount} 个模型` : "正在读取"}</div>
            </div>

            <div className="grid gap-1 text-xs text-muted-foreground">
              <span>本地定价文件</span>
              <code className="min-w-0 break-all font-mono text-[12px] text-foreground">
                {syncState?.filePath ?? "正在读取本地文件路径"}
              </code>
            </div>

            {syncState?.lastSyncError ? (
              <div className="text-xs text-danger-foreground">上次同步失败：{syncState.lastSyncError}</div>
            ) : null}

            <div className="flex flex-wrap justify-end gap-2 border-t border-border pt-3">
              <Button disabled={!syncState || openingCatalogDirectory} variant="outline" onClick={() => void openCatalogDirectory()}>
                <FolderOpen className="h-4 w-4" />
                {openingCatalogDirectory ? "打开中" : "打开目录"}
              </Button>
              <Button disabled={!syncState || reloadingCatalog} variant="outline" onClick={() => void refresh(true)}>
                <RefreshCw className={`h-4 w-4 ${reloadingCatalog ? "animate-spin" : ""}`} />
                {reloadingCatalog ? "加载中" : "重新加载文件"}
              </Button>
              <Button disabled={!syncState || syncing || savingSyncConfig} variant="outline" onClick={openModelPicker}>
                <SlidersHorizontal className="h-4 w-4" />
                选择自动同步模型
              </Button>
              <Button disabled={!syncState || syncing} onClick={() => void runModelPriceSync(true)}>
                <RefreshCw className={`h-4 w-4 ${syncing ? "animate-spin" : ""}`} />
                {syncing ? "同步中" : "立即同步"}
              </Button>
            </div>
          </div>
        </section>
        <SectionCard
          title="价格清单"
          action={
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
              <span>{metrics.total} 个模型</span>
              <span>{metrics.enabled} 个启用</span>
              <span>{metrics.builtIn} 个内置</span>
            </div>
          }
          contentClassName="overflow-hidden rounded-none border-0 bg-transparent p-0 shadow-none"
        >
          <div className="border-b border-border bg-surface px-3 py-2">
            <div className="relative min-w-[220px]">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/70" />
              <input
                aria-label="搜索模型基准价格"
                className="h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface pl-8 pr-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30"
                placeholder="搜索模型或显示名称"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </div>
          </div>

          {!loading && visibleRows.length === 0 ? (
            <div className="px-2.5 py-8 text-center text-sm text-muted-foreground">
              暂无符合条件的模型基准价格
            </div>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full min-w-[900px] table-fixed text-left text-[13px]">
                <TableColumnHeaderRow />
                <tbody className="divide-y divide-border">
                  {visibleRows.map((row) => (
                    <tr key={row.id} className="h-[68px] text-foreground transition-colors hover:bg-surface-subtle">
                      <td className="px-4 font-mono text-[12px] font-medium text-foreground">{row.model}</td>
                      <td className="px-4 text-foreground">{modelPriceDisplayName(row)}</td>
                      <td className="px-3 text-right"><PriceListValue value={row.inputPrice} /></td>
                      <td className="px-3 text-right"><PriceListValue value={row.outputPrice} /></td>
                      <td className="px-3 text-right"><PriceListValue value={row.cacheReadPrice} /></td>
                      <td className="px-3 text-right"><PriceListValue value={row.cacheCreationPrice} /></td>
                      <td className="px-3 text-right">
                        <div className="flex items-center justify-end gap-1">
                          <IconButton label={`编辑 ${row.model}`} onClick={() => openEditDialog(row)}>
                            <Pencil className="h-3.5 w-3.5 text-muted-foreground" />
                          </IconButton>
                          <IconButton
                            label={`删除 ${row.model}`}
                            disabled={savingKeys.has(`${row.id}:delete`)}
                            onClick={() => void deleteRow(row)}
                          >
                            <Trash2 className="h-3.5 w-3.5 text-danger-foreground" />
                          </IconButton>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </SectionCard>

        {displayError && <div className="text-sm text-danger-foreground">{displayError}</div>}
      </div>

      <Dialog
        open={modelPickerOpen}
        title="选择自动同步定价的模型"
        description="通过搜索和供应商筛选需要自动更新的模型；选择结果保存在本地。"
        className="max-w-[920px]"
        footer={
          <div className="flex justify-end gap-2">
            <Button disabled={savingSyncConfig} variant="outline" onClick={() => setModelPickerOpen(false)}>
              取消
            </Button>
            <Button
              disabled={savingSyncConfig || !syncState || modelPriceCatalogLoading || Boolean(modelPriceCatalogError)}
              onClick={() => void saveModelSelection()}
            >
              {savingSyncConfig ? "保存中" : "保存选择"}
            </Button>
          </div>
        }
        onClose={() => setModelPickerOpen(false)}
      >
        {modelPriceCatalogLoading ? (
          <div className="flex min-h-[260px] items-center justify-center gap-2 px-5 py-10 text-sm text-muted-foreground">
            <RefreshCw className="h-4 w-4 animate-spin" />
            正在读取完整模型目录…
          </div>
        ) : modelPriceCatalogError ? (
          <div className="grid min-h-[260px] place-content-center justify-items-center gap-3 px-5 py-10 text-center">
            <div className="text-sm text-danger-foreground">读取完整模型目录失败：{modelPriceCatalogError}</div>
            <Button variant="outline" onClick={() => void loadModelPriceCatalog()}>
              <RefreshCw className="h-4 w-4" />
              重试
            </Button>
          </div>
        ) : modelSelectionOptions.length === 0 ? (
          <div className="px-5 py-10 text-center text-sm text-muted-foreground">
            暂无可选模型，请先在页面外点击“立即同步”获取完整模型目录。
          </div>
        ) : (
          <div className="grid gap-3 p-5">
            <div className="flex flex-wrap items-center justify-between gap-4 rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2.5">
              <div className="min-w-0">
                <div className="text-sm font-medium text-foreground">自动包含常用模型</div>
                <div className="mt-0.5 text-xs text-muted-foreground">
                  一键选择 {commonModelSelectionCount} 个近期 Claude、GPT、Gemini、Grok、DeepSeek、Qwen、MiMo、LongCat、Kimi、MiniMax 和 GLM 模型。
                </div>
              </div>
              <SwitchControl
                ariaLabel="自动包含常用模型"
                checked={includeCommonModelsDraft && allCommonModelsSelected}
                showLabel={false}
                onCheckedChange={() => setCommonModelsSelected(!(includeCommonModelsDraft && allCommonModelsSelected))}
              />
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <SelectControl
                ariaLabel="筛选自动同步模型供应商"
                className="min-w-[180px]"
                options={modelPickerProviderOptions}
                value={modelPickerProvider}
                onChange={setModelPickerProvider}
              />
              <div className="relative min-w-[240px] flex-1">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground/70" />
                <input
                  aria-label="搜索自动同步模型"
                  className="h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface pl-8 pr-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30"
                  placeholder="搜索模型或供应商（全量搜索）..."
                  value={modelPickerQuery}
                  onChange={(event) => setModelPickerQuery(event.target.value)}
                />
              </div>
              <Button
                disabled={filteredModelSelectionOptions.length === 0}
                variant="outline"
                onClick={() => setFilteredModelsSelected(true)}
              >
                全选筛选结果（{filteredModelSelectionOptions.length}）
              </Button>
              <Button
                disabled={filteredModelSelectionOptions.length === 0}
                variant="outline"
                onClick={() => setFilteredModelsSelected(false)}
              >
                清空筛选结果
              </Button>
            </div>

            <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
              <span>已选择 {selectedModelKeysDraft.size} 个模型</span>
              <span>归一化模型 ID 相同的选项只会导入一次。</span>
            </div>

            <div className="max-h-[calc(100vh-390px)] min-h-[220px] overflow-y-auto rounded-[var(--surface-radius)] border border-border">
              {filteredModelSelectionOptions.length === 0 ? (
                <div className="flex min-h-[220px] items-center justify-center px-4 text-sm text-muted-foreground">
                  没有匹配的模型
                </div>
              ) : (
                <div className="divide-y divide-border">
                  {visibleModelSelectionOptions.map((option) => {
                    const checked = selectedModelKeysDraft.has(option.key);
                    return (
                      <label
                        key={option.key}
                        className={`flex min-w-0 cursor-pointer items-center gap-3 px-3 py-2 transition ${checked ? "bg-selected" : "hover:bg-hover"}`}
                      >
                        <input
                          checked={checked}
                          className="h-4 w-4 shrink-0 accent-primary"
                          type="checkbox"
                          onChange={() => toggleModelSelection(option)}
                        />
                        <span className="min-w-0 flex-1">
                          <span className="flex min-w-0 items-center gap-2">
                            <span className="truncate text-sm font-medium text-foreground">{option.name}</span>
                            <span className="shrink-0 text-xs text-muted-foreground">{option.provider}</span>
                            {option.releaseDate ? (
                              <span className="shrink-0 text-[10px] text-muted-foreground/75">{option.releaseDate}</span>
                            ) : null}
                            {option.common ? (
                              <span className="shrink-0 rounded bg-selected px-1.5 py-0.5 text-[10px] text-primary">常用</span>
                            ) : null}
                          </span>
                          <span className="mt-0.5 block truncate font-mono text-xs text-muted-foreground" title={option.model}>
                            {option.model}
                          </span>
                        </span>
                        <span className="hidden shrink-0 grid-cols-4 gap-3 text-right sm:grid">
                          <ModelPickerPrice label="输入成本" value={option.inputPrice} />
                          <ModelPickerPrice label="输出成本" value={option.outputPrice} />
                          <ModelPickerPrice label="缓存命中" value={option.cacheReadPrice} />
                          <ModelPickerPrice label="缓存创建" value={option.cacheCreationPrice} />
                        </span>
                      </label>
                    );
                  })}
                  {filteredModelSelectionOptions.length > visibleModelSelectionOptions.length ? (
                    <div className="px-3 py-2 text-center text-xs text-muted-foreground">
                      当前显示前 {visibleModelSelectionOptions.length} 条，共 {filteredModelSelectionOptions.length} 条；请使用搜索缩小范围。
                    </div>
                  ) : null}
                </div>
              )}
            </div>
          </div>
        )}
      </Dialog>

    </PageScaffold>
  );
}

function TableColumnHeaderRow() {
  return (
    <thead>
      <tr className="h-10 border-b border-border bg-surface text-xs font-medium text-muted-foreground">
        <th className="w-[29%] px-4">模型</th>
        <th className="w-[22%] px-4">显示名称</th>
        <th className="w-[11%] px-3 text-right">输入成本</th>
        <th className="w-[11%] px-3 text-right">输出成本</th>
        <th className="w-[11%] px-3 text-right">缓存命中</th>
        <th className="w-[11%] px-3 text-right">缓存创建</th>
        <th className="w-[5%] px-3 text-right">操作</th>
      </tr>
    </thead>
  );
}

function PriceListValue({ value }: { value: number | null }) {
  return (
    <span className="font-mono text-xs tabular-nums text-foreground">
      {value === null ? "—" : `$${formatCompactPrice(value)}`}
    </span>
  );
}

function Field({
  label,
  value,
  numeric,
  placeholder,
  onChange,
}: {
  label: string;
  value: string;
  numeric?: boolean;
  placeholder?: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="grid gap-1 text-xs font-medium text-muted-foreground">
      <span>{label}</span>
      <input
        aria-label={label}
        className="h-8 min-w-0 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30"
        min={numeric ? "0" : undefined}
        placeholder={placeholder}
        step={numeric ? "0.0001" : undefined}
        type={numeric ? "number" : "text"}
        value={value}
        onChange={(event) => onChange(event.target.value)}
      />
    </label>
  );
}

function draftToInput(draft: DraftRow) {
  return {
    id: draft.id,
    provider: draft.provider.trim(),
    model: draft.model.trim(),
    inputPrice: draft.inputPrice.trim() === "" ? null : Number(draft.inputPrice),
    outputPrice: draft.outputPrice.trim() === "" ? null : Number(draft.outputPrice),
    inputPricePriority: nullableNumber(draft.inputPricePriority),
    outputPricePriority: nullableNumber(draft.outputPricePriority),
    cacheCreationPrice: nullableNumber(draft.cacheCreationPrice),
    cacheCreationPricePriority: nullableNumber(draft.cacheCreationPricePriority),
    cacheCreationPriceAbove1Hr: nullableNumber(draft.cacheCreationPriceAbove1Hr),
    cacheReadPrice: nullableNumber(draft.cacheReadPrice),
    cacheReadPricePriority: nullableNumber(draft.cacheReadPricePriority),
    longContextInputTokenThreshold: nullableInteger(draft.longContextInputTokenThreshold),
    longContextInputCostMultiplier: nullableNumber(draft.longContextInputCostMultiplier),
    longContextOutputCostMultiplier: nullableNumber(draft.longContextOutputCostMultiplier),
    supportsServiceTier: draft.supportsServiceTier,
    supportsPromptCaching: draft.supportsPromptCaching,
    currency: draft.currency.trim() || "USD",
    unit: draft.unit.trim() || "M",
    sourceUrl: draft.sourceUrl.trim(),
    sourceLabel: draft.sourceLabel.trim() || "Manual",
    sourceCheckedAt: draft.sourceCheckedAt.trim() === "" ? null : draft.sourceCheckedAt,
    enabled: draft.enabled,
    builtIn: draft.builtIn,
    note: draft.note.trim() === "" ? null : draft.note,
  };
}

function rowToDraft(row: ModelBasePrice): DraftRow {
  return {
    id: row.id,
    provider: row.provider,
    model: row.model,
    inputPrice: formatPriceInput(row.inputPrice),
    outputPrice: formatPriceInput(row.outputPrice),
    inputPricePriority: formatPriceInput(row.inputPricePriority),
    outputPricePriority: formatPriceInput(row.outputPricePriority),
    cacheCreationPrice: formatPriceInput(row.cacheCreationPrice),
    cacheCreationPricePriority: formatPriceInput(row.cacheCreationPricePriority),
    cacheCreationPriceAbove1Hr: formatPriceInput(row.cacheCreationPriceAbove1Hr),
    cacheReadPrice: formatPriceInput(row.cacheReadPrice),
    cacheReadPricePriority: formatPriceInput(row.cacheReadPricePriority),
    longContextInputTokenThreshold: row.longContextInputTokenThreshold?.toString() ?? "",
    longContextInputCostMultiplier: formatPriceInput(row.longContextInputCostMultiplier),
    longContextOutputCostMultiplier: formatPriceInput(row.longContextOutputCostMultiplier),
    supportsServiceTier: row.supportsServiceTier,
    supportsPromptCaching: row.supportsPromptCaching,
    currency: row.currency,
    unit: row.unit,
    sourceUrl: row.sourceUrl,
    sourceLabel: row.sourceLabel,
    sourceCheckedAt: row.sourceCheckedAt ?? "",
    enabled: row.enabled,
    builtIn: row.builtIn,
    note: row.note ?? "",
  };
}

function nullableNumber(value: string) {
  return value.trim() === "" ? null : Number(value);
}

function nullableInteger(value: string) {
  return value.trim() === "" ? null : Math.trunc(Number(value));
}

function upsertRow(rows: ModelBasePrice[], row: ModelBasePrice) {
  const found = rows.some((item) => item.id === row.id);
  const nextRows = found ? rows.map((item) => (item.id === row.id ? row : item)) : [...rows, row];
  return nextRows.sort(compareRows);
}

function compareRows(left: ModelBasePrice, right: ModelBasePrice) {
  return left.model.localeCompare(right.model);
}

function modelPriceDisplayName(row: ModelBasePrice) {
  const prefix = row.note?.split(";", 1)[0]?.trim();
  if (prefix && !/^usd per\b/i.test(prefix)) {
    return prefix;
  }
  return row.model;
}

function replaceModelPriceDisplayName(note: string, displayName: string) {
  const normalizedDisplayName = displayName.trim();
  const separator = note.indexOf(";");
  const suffix = separator >= 0 ? note.slice(separator + 1).trim() : "";
  return suffix ? `${normalizedDisplayName}; ${suffix}` : normalizedDisplayName;
}

function formatPriceInput(value: number | null) {
  return value === null ? "" : String(value);
}

function ModelPickerPrice({ label, value }: { label: string; value: number | null }) {
  return (
    <span className="w-[68px]">
      <span className="block text-[10px] text-muted-foreground">{label}</span>
      <span className="block font-mono text-xs text-foreground">
        {value === null ? "—" : `$${formatCompactPrice(value)}`}
      </span>
    </span>
  );
}

function formatCompactPrice(value: number) {
  if (value === 0) {
    return "0";
  }
  if (value < 0.01) {
    return value.toPrecision(2);
  }
  return Number.isInteger(value) ? value.toFixed(0) : String(Number(value.toFixed(6)));
}

function formatLocalDate(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function formatSyncTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function normalizeCatalogModelIdForPricing(modelId: string) {
  const segments = modelId.split("/");
  const afterSlash = segments[segments.length - 1] ?? modelId;
  const beforeColon = afterSlash.split(":", 1)[0] ?? "";
  let normalized = beforeColon.trim().replace(/@/g, "-").toLowerCase();
  if (normalized.endsWith("[1m]")) {
    normalized = normalized.slice(0, -"[1m]".length).trim();
  }
  return normalized;
}

function stableModelPriceId(model: string) {
  const suffix = [...model]
    .map((character) => /[a-z0-9]/i.test(character) ? character.toLowerCase() : "-")
    .join("")
    .replace(/^-+|-+$/g, "");
  return `builtin-${suffix}`;
}

function formatImportedPrice(value: number | null) {
  return value === null ? "0" : String(value);
}

function normalizeModelKey(value: string) {
  return value.trim().toLowerCase();
}

function addMissingModelSelectionOption(
  options: Map<string, ModelSelectionOption>,
  rawKey: string,
  common: boolean,
) {
  const key = normalizeModelKey(rawKey);
  if (!key || options.has(key)) {
    return;
  }
  const separator = key.indexOf("/");
  options.set(key, {
    key,
    provider: separator > 0 ? key.slice(0, separator) : "models.dev",
    model: separator > 0 ? key.slice(separator + 1) : key,
    name: separator > 0 ? key.slice(separator + 1) : key,
    releaseDate: "",
    common,
    inputPrice: null,
    outputPrice: null,
    cacheReadPrice: null,
    cacheCreationPrice: null,
  });
}

function getCommonModelKeys(options: ModelSelectionOption[]) {
  const keys = new Set<string>();
  for (const rule of commonModelFamilyRules) {
    let count = 0;
    for (const option of options) {
      if (!rule.providers.has(option.provider.toLowerCase()) || !rule.matches(option.model.toLowerCase())) {
        continue;
      }
      keys.add(option.key);
      count += 1;
      if (count >= COMMON_MODEL_LIMIT_PER_FAMILY) {
        break;
      }
    }
  }
  return keys;
}
