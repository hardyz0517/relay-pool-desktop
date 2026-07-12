import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { RefreshCw, RotateCcw, Save } from "lucide-react";
import {
  Button,
  SectionCard,
  StatusBadge,
  useToast,
} from "@/components/ui";
import {
  getSettings,
  SETTINGS_UPDATED_EVENT,
  updateSettings,
} from "@/lib/api/settings";
import { readError } from "@/lib/errors";
import {
  appSettingsToUpdateInput,
  DEFAULT_SCHEDULER_ADVANCED_SETTINGS,
  type AppSettings,
} from "@/lib/types/settings";
import {
  createLocalRoutingSettingsDraft,
  parseLocalRoutingBoundaryDraft,
  parseLocalRoutingSchedulerDraft,
  ROUTING_GROUP_PRESET_OPTIONS,
  type LocalRoutingSettingsDraft,
  type LocalRoutingSettingsErrorKey,
  type LocalRoutingSettingsErrors,
  type RoutingGroupPreset,
  type SchedulerBooleanField,
  type SchedulerNumericField,
} from "./localRoutingSettingsForm";
import {
  isBaseWeightField,
  LocalRoutingBoundaryFields,
  LocalRoutingSchedulerFields,
} from "./LocalRoutingSettingsFields";

type SaveState = "idle" | "saving" | "saved" | "error";

type VisibleSaveState = SaveState | "dirty";

const saveStateLabels: Record<VisibleSaveState, string> = {
  idle: "未修改",
  dirty: "待保存",
  saving: "保存中",
  saved: "已保存",
  error: "保存失败",
};

const saveStateTones: Record<VisibleSaveState, "info" | "warning" | "healthy" | "error"> = {
  idle: "info",
  dirty: "warning",
  saving: "warning",
  saved: "healthy",
  error: "error",
};

export function LocalRoutingSettingsEditor() {
  const toast = useToast();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [draft, setDraft] = useState<LocalRoutingSettingsDraft | null>(null);
  const [savedDraft, setSavedDraft] = useState<LocalRoutingSettingsDraft | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [boundarySaveState, setBoundarySaveState] = useState<SaveState>("idle");
  const [boundarySavePending, setBoundarySavePending] = useState(false);
  const [boundarySaveError, setBoundarySaveError] = useState<string | null>(null);
  const [schedulerSaveState, setSchedulerSaveState] = useState<SaveState>("idle");
  const [schedulerSaveError, setSchedulerSaveError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<LocalRoutingSettingsErrors>({});
  const settingsRef = useRef<AppSettings | null>(null);
  const loadOperationRef = useRef(0);
  const boundarySaveOperationRef = useRef(0);
  const boundaryDraftVersionRef = useRef(0);
  const schedulerSaveOperationRef = useRef(0);
  const boundarySaveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    void loadCurrentSettings();
    return () => {
      loadOperationRef.current += 1;
      boundarySaveOperationRef.current += 1;
      boundaryDraftVersionRef.current += 1;
      schedulerSaveOperationRef.current += 1;
      if (boundarySaveTimeoutRef.current) {
        clearTimeout(boundarySaveTimeoutRef.current);
      }
    };
  }, []);

  function applySettings(nextSettings: AppSettings) {
    settingsRef.current = nextSettings;
    setSettings(nextSettings);
  }

  const schedulerDirty = useMemo(
    () => {
      if (!draft || !savedDraft) {
        return false;
      }
      return Object.entries(draft.scheduler).some(
        ([field, value]) =>
          field !== "multiplierMinConfidence" &&
          savedDraft.scheduler[field as keyof typeof savedDraft.scheduler] !== value,
      );
    },
    [draft, savedDraft],
  );
  const visibleSchedulerSaveState: VisibleSaveState =
    schedulerSaveState === "saving" || schedulerSaveState === "error"
      ? schedulerSaveState
      : schedulerDirty
        ? "dirty"
        : schedulerSaveState;

  async function loadCurrentSettings() {
    const operationId = loadOperationRef.current + 1;
    loadOperationRef.current = operationId;
    setLoading(true);
    setLoadError(null);
    try {
      const nextSettings = await getSettings();
      if (operationId !== loadOperationRef.current) {
        return;
      }
      const nextDraft = createLocalRoutingSettingsDraft(nextSettings);
      applySettings(nextSettings);
      setDraft(nextDraft);
      setSavedDraft(nextDraft);
      setFieldErrors({});
      setBoundarySaveError(null);
      setSchedulerSaveError(null);
      setBoundarySaveState("idle");
      setBoundarySavePending(false);
      setSchedulerSaveState("idle");
      boundaryDraftVersionRef.current += 1;
    } catch (requestError) {
      if (operationId !== loadOperationRef.current) {
        return;
      }
      setLoadError(readError(requestError));
    } finally {
      if (operationId === loadOperationRef.current) {
        setLoading(false);
      }
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const currentSettings = settingsRef.current ?? settings;
    if (
      !currentSettings ||
      !draft ||
      schedulerSaveState === "saving" ||
      boundarySaveState === "saving" ||
      boundarySavePending
    ) {
      return;
    }
    const parsed = parseLocalRoutingSchedulerDraft(draft);
    if (!parsed.ok) {
      setFieldErrors((current) => ({ ...current, ...parsed.errors }));
      setSchedulerSaveState("error");
      setSchedulerSaveError("请修正标记的参数");
      requestAnimationFrame(() => {
        document.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
      });
      return;
    }

    const operationId = schedulerSaveOperationRef.current + 1;
    schedulerSaveOperationRef.current = operationId;
    setSchedulerSaveState("saving");
    setSchedulerSaveError(null);
    setFieldErrors({});
    try {
      const nextSettings = await updateSettings({
        ...appSettingsToUpdateInput(currentSettings),
        defaultRoutingStrategy: "automatic_balanced",
        maxRateMultiplier: currentSettings.maxRateMultiplier,
        defaultRoutingGroupFilter: currentSettings.defaultRoutingGroupFilter,
        schedulerAdvancedSettings: parsed.value.schedulerAdvancedSettings,
      });
      if (operationId !== schedulerSaveOperationRef.current) {
        return;
      }
      const nextDraft = createLocalRoutingSettingsDraft(nextSettings);
      applySettings(nextSettings);
      setDraft((current) => (current ? { ...current, scheduler: nextDraft.scheduler } : nextDraft));
      setSavedDraft((current) => (current ? { ...current, scheduler: nextDraft.scheduler } : nextDraft));
      setSchedulerSaveState("saved");
      window.dispatchEvent(new Event(SETTINGS_UPDATED_EVENT));
      toast.success("调度参数已保存");
    } catch (requestError) {
      if (operationId !== schedulerSaveOperationRef.current) {
        return;
      }
      const message = readError(requestError);
      setSchedulerSaveState("error");
      setSchedulerSaveError(message);
      toast.error("保存调度参数失败", message);
    }
  }

  function updateDraft(update: (current: LocalRoutingSettingsDraft) => LocalRoutingSettingsDraft) {
    setDraft((current) => (current ? update(current) : current));
    setSchedulerSaveState((current) => (current === "saving" ? current : "idle"));
    setSchedulerSaveError(null);
  }

  function updateNumericField(field: SchedulerNumericField, value: string) {
    clearFieldError(field);
    if (isBaseWeightField(field)) {
      clearFieldError("baseWeights");
    }
    updateDraft((current) => ({
      ...current,
      scheduler: { ...current.scheduler, [field]: value },
    }));
  }

  function updateBoundaryNumericField(field: SchedulerNumericField, value: string) {
    if (field !== "multiplierMinConfidence") {
      updateNumericField(field, value);
      return;
    }
    if (!draft) {
      return;
    }
    clearFieldError(field);
    boundaryDraftVersionRef.current += 1;
    const nextDraft = {
      ...draft,
      scheduler: { ...draft.scheduler, [field]: value },
    };
    setDraft(nextDraft);
    setBoundarySaveState((current) => (current === "saving" ? current : "idle"));
    setBoundarySaveError(null);
    queueBoundaryAutoSave(nextDraft);
  }

  function updateBooleanField(field: SchedulerBooleanField) {
    clearFieldError(field);
    updateDraft((current) => ({
      ...current,
      scheduler: { ...current.scheduler, [field]: !current.scheduler[field] },
    }));
  }

  function clearFieldError(field: LocalRoutingSettingsErrorKey) {
    setFieldErrors((current) => {
      if (!(field in current)) {
        return current;
      }
      const next = { ...current };
      delete next[field];
      return next;
    });
  }

  function queueBoundaryAutoSave(nextDraft: LocalRoutingSettingsDraft, delayMs = 400) {
    const draftVersion = boundaryDraftVersionRef.current;
    if (boundarySaveTimeoutRef.current) {
      clearTimeout(boundarySaveTimeoutRef.current);
      boundarySaveTimeoutRef.current = null;
    }
    if (delayMs === 0) {
      setBoundarySavePending(false);
      void handleBoundaryAutoSave(nextDraft, draftVersion);
      return;
    }
    setBoundarySavePending(true);
    boundarySaveTimeoutRef.current = setTimeout(() => {
      boundarySaveTimeoutRef.current = null;
      void handleBoundaryAutoSave(nextDraft, draftVersion);
    }, delayMs);
  }

  async function handleBoundaryAutoSave(nextDraft: LocalRoutingSettingsDraft, draftVersion: number) {
    const currentSettings = settingsRef.current ?? settings;
    setBoundarySavePending(false);
    if (!currentSettings) {
      return;
    }
    const parsed = parseLocalRoutingBoundaryDraft(nextDraft);
    if (!parsed.ok) {
      setFieldErrors((current) => ({ ...current, ...parsed.errors }));
      setBoundarySaveState("error");
      setBoundarySaveError("请修正标记的边界参数");
      return;
    }

    const operationId = boundarySaveOperationRef.current + 1;
    boundarySaveOperationRef.current = operationId;
    setBoundarySaveState("saving");
    setBoundarySaveError(null);
    setFieldErrors((current) => {
      if (!("maxRateMultiplier" in current) && !("multiplierMinConfidence" in current)) {
        return current;
      }
      const next = { ...current };
      delete next.maxRateMultiplier;
      delete next.multiplierMinConfidence;
      return next;
    });

    try {
      const nextSettings = await updateSettings({
        ...appSettingsToUpdateInput(currentSettings),
        defaultRoutingStrategy: "automatic_balanced",
        maxRateMultiplier: parsed.value.maxRateMultiplier,
        defaultRoutingGroupFilter: parsed.value.defaultRoutingGroupFilter,
        schedulerAdvancedSettings: {
          ...currentSettings.schedulerAdvancedSettings,
          ...parsed.value.schedulerAdvancedPatch,
        },
      });
      if (
        operationId !== boundarySaveOperationRef.current ||
        draftVersion !== boundaryDraftVersionRef.current
      ) {
        return;
      }
      const nextSavedDraft = createLocalRoutingSettingsDraft(nextSettings);
      const savedBoundary = {
        maxRateMultiplier: nextSavedDraft.maxRateMultiplier,
        defaultRoutingGroupPreset: nextSavedDraft.defaultRoutingGroupPreset,
        currentRoutingGroupFilter: nextSavedDraft.currentRoutingGroupFilter,
      };
      const savedBoundaryScheduler = {
        multiplierMinConfidence: nextSavedDraft.scheduler.multiplierMinConfidence,
      };
      applySettings(nextSettings);
      setDraft((current) =>
        current
          ? {
              ...current,
              ...savedBoundary,
              scheduler: { ...current.scheduler, ...savedBoundaryScheduler },
            }
          : nextSavedDraft,
      );
      setSavedDraft((current) =>
        current
          ? {
              ...current,
              ...savedBoundary,
              scheduler: { ...current.scheduler, ...savedBoundaryScheduler },
            }
          : nextSavedDraft,
      );
      setBoundarySaveState("saved");
      window.dispatchEvent(new Event(SETTINGS_UPDATED_EVENT));
      toast.success("路由边界已保存");
    } catch (requestError) {
      if (
        operationId !== boundarySaveOperationRef.current ||
        draftVersion !== boundaryDraftVersionRef.current
      ) {
        return;
      }
      const message = readError(requestError);
      setBoundarySaveState("error");
      setBoundarySaveError(message);
      toast.error("保存路由边界失败", message);
    }
  }

  function updateMaxRateMultiplier(maxRateMultiplier: string) {
    if (!draft) {
      return;
    }
    clearFieldError("maxRateMultiplier");
    boundaryDraftVersionRef.current += 1;
    const nextDraft = { ...draft, maxRateMultiplier };
    setDraft(nextDraft);
    setBoundarySaveState((current) => (current === "saving" ? current : "idle"));
    setBoundarySaveError(null);
    queueBoundaryAutoSave(nextDraft);
  }

  function updateRoutingGroupPreset(defaultRoutingGroupPreset: RoutingGroupPreset) {
    if (!draft) {
      return;
    }
    boundaryDraftVersionRef.current += 1;
    const nextDraft = { ...draft, defaultRoutingGroupPreset };
    setDraft(nextDraft);
    setBoundarySaveState((current) => (current === "saving" ? current : "idle"));
    setBoundarySaveError(null);
    queueBoundaryAutoSave(nextDraft, 0);
  }

  function resetSchedulerDefaults() {
    if (!settings) {
      return;
    }
    const defaultSchedulerDraft = createLocalRoutingSettingsDraft({
      ...settings,
      schedulerAdvancedSettings: DEFAULT_SCHEDULER_ADVANCED_SETTINGS,
    }).scheduler;
    setFieldErrors({});
    updateDraft((current) => ({ ...current, scheduler: defaultSchedulerDraft }));
  }

  if (loading && !draft) {
    return (
      <SectionCard title="自动调度">
        <div className="text-sm text-muted-foreground">正在加载设置...</div>
      </SectionCard>
    );
  }

  if (!settings || !draft) {
    return (
      <SectionCard title="自动调度">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <span className="text-sm text-rose-700">{loadError ?? "设置加载失败"}</span>
          <Button type="button" variant="outline" onClick={() => void loadCurrentSettings()}>
            <RefreshCw className="h-4 w-4" />
            重试
          </Button>
        </div>
      </SectionCard>
    );
  }

  const groupOptions = draft.defaultRoutingGroupPreset === "current_specific"
    ? [
        { value: "current_specific" as const, label: "指定分组（当前）" },
        ...ROUTING_GROUP_PRESET_OPTIONS,
      ]
    : [...ROUTING_GROUP_PRESET_OPTIONS];
  const schedulerDisabled = loading || schedulerSaveState === "saving" || boundarySaveState === "saving" || boundarySavePending;
  const boundaryDisabled = loading || schedulerSaveState === "saving";

  return (
    <form className="grid gap-3" onSubmit={(event) => void handleSubmit(event)}>
      <SectionCard
        title="路由边界"
        action={
          boundarySaveState === "idle" ? null : (
            <StatusBadge tone={saveStateTones[boundarySaveState]}>
              {saveStateLabels[boundarySaveState]}
            </StatusBadge>
          )
        }
        contentClassName="p-0"
      >
        <LocalRoutingBoundaryFields
          disabled={boundaryDisabled}
          draft={draft}
          errors={fieldErrors}
          groupOptions={groupOptions}
          onGroupPresetChange={updateRoutingGroupPreset}
          onMaxRateMultiplierChange={updateMaxRateMultiplier}
          onBooleanChange={updateBooleanField}
          onNumericChange={updateBoundaryNumericField}
        />
        {boundarySaveError ? (
          <div className="border-t border-rose-100 bg-rose-50 px-4 py-2 text-xs text-rose-700">
            {boundarySaveError}
          </div>
        ) : null}
      </SectionCard>

      <SectionCard
        title="自动调度参数"
        action={
          <div className="flex flex-wrap items-center justify-end gap-2">
            <StatusBadge tone={saveStateTones[visibleSchedulerSaveState]}>
              {saveStateLabels[visibleSchedulerSaveState]}
            </StatusBadge>
            <Button disabled={schedulerDisabled} type="button" variant="outline" onClick={resetSchedulerDefaults}>
              <RotateCcw className="h-4 w-4" />
              恢复默认
            </Button>
            <Button disabled={schedulerDisabled || !schedulerDirty} type="submit">
              <Save className="h-4 w-4" />
              保存设置
            </Button>
          </div>
        }
        contentClassName="p-0"
      >
        <LocalRoutingSchedulerFields
          draft={draft}
          disabled={schedulerDisabled}
          errors={fieldErrors}
          onNumericChange={updateNumericField}
          onBooleanChange={updateBooleanField}
        />
        {schedulerSaveError ? (
          <div className="border-t border-rose-100 bg-rose-50 px-4 py-2 text-xs text-rose-700">
            {schedulerSaveError}
          </div>
        ) : null}
      </SectionCard>
    </form>
  );
}
