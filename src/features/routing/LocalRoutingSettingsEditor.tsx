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
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
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

type LocalRoutingSettingsEditorProps = {
  workspace: LocalRoutingWorkspace | null;
};

export function LocalRoutingSettingsEditor({ workspace }: LocalRoutingSettingsEditorProps) {
  const toast = useToast();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [draft, setDraft] = useState<LocalRoutingSettingsDraft | null>(null);
  const [savedDraft, setSavedDraft] = useState<LocalRoutingSettingsDraft | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [boundarySaveState, setBoundarySaveState] = useState<SaveState>("idle");
  const [boundarySaveError, setBoundarySaveError] = useState<string | null>(null);
  const [schedulerSaveState, setSchedulerSaveState] = useState<SaveState>("idle");
  const [schedulerSaveError, setSchedulerSaveError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<LocalRoutingSettingsErrors>({});
  const settingsRef = useRef<AppSettings | null>(null);
  const loadOperationRef = useRef(0);
  const boundarySaveOperationRef = useRef(0);
  const schedulerSaveOperationRef = useRef(0);

  useEffect(() => {
    void loadCurrentSettings();
    return () => {
      loadOperationRef.current += 1;
      boundarySaveOperationRef.current += 1;
      schedulerSaveOperationRef.current += 1;
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

  const boundaryDirty = useMemo(() => {
    if (!draft || !savedDraft) {
      return false;
    }
    return (
      draft.maxRateLimitEnabled !== savedDraft.maxRateLimitEnabled ||
      draft.maxRateMultiplier !== savedDraft.maxRateMultiplier ||
      draft.defaultRoutingGroupPreset !== savedDraft.defaultRoutingGroupPreset ||
      draft.lowBalanceThresholdCny !== savedDraft.lowBalanceThresholdCny ||
      draft.allowDepletedFallback !== savedDraft.allowDepletedFallback ||
      draft.scheduler.multiplierMinConfidence !== savedDraft.scheduler.multiplierMinConfidence
    );
  }, [draft, savedDraft]);

  const visibleSchedulerSaveState: VisibleSaveState =
    schedulerSaveState === "saving" || schedulerSaveState === "error"
      ? schedulerSaveState
      : schedulerDirty
        ? "dirty"
        : schedulerSaveState;
  const visibleBoundarySaveState: VisibleSaveState =
    boundarySaveState === "saving" || boundarySaveState === "error"
      ? boundarySaveState
      : boundaryDirty
        ? "dirty"
        : boundarySaveState;
  const candidateCount = workspace?.summary.candidateCount ?? 0;
  const previewEligibleCandidateCount = workspace?.summary.previewEligibleCandidateCount ?? 0;

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
      setSchedulerSaveState("idle");
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
      boundarySaveState === "saving"
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
        lowBalanceThresholdCny: currentSettings.lowBalanceThresholdCny,
        allowDepletedFallback: currentSettings.allowDepletedFallback,
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

  async function handleBoundarySave() {
    const currentSettings = settingsRef.current ?? settings;
    if (!currentSettings || !draft || boundarySaveState === "saving") {
      return;
    }
    const parsed = parseLocalRoutingBoundaryDraft(draft);
    if (!parsed.ok) {
      setFieldErrors((current) => ({ ...current, ...parsed.errors }));
      setBoundarySaveState("error");
      setBoundarySaveError("请修正标记的边界参数");
      requestAnimationFrame(() => {
        document.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
      });
      return;
    }

    const operationId = boundarySaveOperationRef.current + 1;
    boundarySaveOperationRef.current = operationId;
    setBoundarySaveState("saving");
    setBoundarySaveError(null);
    setFieldErrors((current) => {
      const next = { ...current };
      delete next.maxRateMultiplier;
      delete next.lowBalanceThresholdCny;
      delete next.multiplierMinConfidence;
      return next;
    });

    try {
      const nextSettings = await updateSettings({
        ...appSettingsToUpdateInput(currentSettings),
        defaultRoutingStrategy: "automatic_balanced",
        maxRateMultiplier: parsed.value.maxRateMultiplier,
        defaultRoutingGroupFilter: parsed.value.defaultRoutingGroupFilter,
        lowBalanceThresholdCny: parsed.value.lowBalanceThresholdCny,
        allowDepletedFallback: parsed.value.allowDepletedFallback,
        schedulerAdvancedSettings: {
          ...currentSettings.schedulerAdvancedSettings,
          ...parsed.value.schedulerAdvancedPatch,
        },
      });
      if (operationId !== boundarySaveOperationRef.current) {
        return;
      }
      const nextSavedDraft = createLocalRoutingSettingsDraft(nextSettings);
      applySettings(nextSettings);
      setDraft((current) =>
        current
          ? {
              ...current,
              maxRateLimitEnabled: nextSavedDraft.maxRateLimitEnabled,
              maxRateMultiplier: nextSavedDraft.maxRateMultiplier,
              defaultRoutingGroupPreset: nextSavedDraft.defaultRoutingGroupPreset,
              currentRoutingGroupFilter: nextSavedDraft.currentRoutingGroupFilter,
              lowBalanceThresholdCny: nextSavedDraft.lowBalanceThresholdCny,
              allowDepletedFallback: nextSavedDraft.allowDepletedFallback,
              scheduler: {
                ...current.scheduler,
                multiplierMinConfidence: nextSavedDraft.scheduler.multiplierMinConfidence,
              },
            }
          : nextSavedDraft,
      );
      setSavedDraft((current) =>
        current
          ? {
              ...current,
              maxRateLimitEnabled: nextSavedDraft.maxRateLimitEnabled,
              maxRateMultiplier: nextSavedDraft.maxRateMultiplier,
              defaultRoutingGroupPreset: nextSavedDraft.defaultRoutingGroupPreset,
              currentRoutingGroupFilter: nextSavedDraft.currentRoutingGroupFilter,
              lowBalanceThresholdCny: nextSavedDraft.lowBalanceThresholdCny,
              allowDepletedFallback: nextSavedDraft.allowDepletedFallback,
              scheduler: {
                ...current.scheduler,
                multiplierMinConfidence: nextSavedDraft.scheduler.multiplierMinConfidence,
              },
            }
          : nextSavedDraft,
      );
      setBoundarySaveState("saved");
      window.dispatchEvent(new Event(SETTINGS_UPDATED_EVENT));
      toast.success("路由边界已保存");
    } catch (requestError) {
      if (operationId !== boundarySaveOperationRef.current) {
        return;
      }
      const message = readError(requestError);
      setBoundarySaveState("error");
      setBoundarySaveError(message);
      toast.error("保存路由边界失败", message);
    }
  }

  function updateDraft(update: (current: LocalRoutingSettingsDraft) => LocalRoutingSettingsDraft) {
    setDraft((current) => (current ? update(current) : current));
    setSchedulerSaveState((current) => (current === "saving" ? current : "idle"));
    setSchedulerSaveError(null);
  }

  function updateBoundaryDraft(
    update: (current: LocalRoutingSettingsDraft) => LocalRoutingSettingsDraft,
  ) {
    setDraft((current) => (current ? update(current) : current));
    setBoundarySaveState((current) => (current === "saving" ? current : "idle"));
    setBoundarySaveError(null);
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
    clearFieldError(field);
    updateBoundaryDraft((current) => ({
      ...current,
      scheduler: { ...current.scheduler, [field]: value },
    }));
  }

  function updateBooleanField(field: SchedulerBooleanField) {
    clearFieldError(field);
    updateDraft((current) => ({
      ...current,
      scheduler: { ...current.scheduler, [field]: !current.scheduler[field] },
    }));
  }

  function updateMaxRateLimitEnabled() {
    clearFieldError("maxRateMultiplier");
    updateBoundaryDraft((current) => ({
      ...current,
      maxRateLimitEnabled: !current.maxRateLimitEnabled,
    }));
  }

  function updateMaxRateMultiplier(maxRateMultiplier: string) {
    clearFieldError("maxRateMultiplier");
    updateBoundaryDraft((current) => ({ ...current, maxRateMultiplier }));
  }

  function updateRoutingGroupPreset(defaultRoutingGroupPreset: RoutingGroupPreset) {
    updateBoundaryDraft((current) => ({ ...current, defaultRoutingGroupPreset }));
  }

  function updateLowBalanceThresholdCny(lowBalanceThresholdCny: string) {
    clearFieldError("lowBalanceThresholdCny");
    updateBoundaryDraft((current) => ({ ...current, lowBalanceThresholdCny }));
  }

  function updateAllowDepletedFallback() {
    updateBoundaryDraft((current) => ({
      ...current,
      allowDepletedFallback: !current.allowDepletedFallback,
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
  const schedulerDisabled = loading || schedulerSaveState === "saving" || boundarySaveState === "saving";
  const boundaryDisabled = loading || schedulerSaveState === "saving" || boundarySaveState === "saving";

  return (
    <form className="grid gap-3" onSubmit={(event) => void handleSubmit(event)}>
      <SectionCard
        title="路由边界"
        action={
          <div className="flex flex-wrap items-center justify-end gap-2">
            <StatusBadge tone={saveStateTones[visibleBoundarySaveState]}>
              {saveStateLabels[visibleBoundarySaveState]}
            </StatusBadge>
            <Button
              disabled={boundaryDisabled || !boundaryDirty}
              type="button"
              onClick={() => void handleBoundarySave()}
            >
              <Save className="h-4 w-4" />
              保存路由边界
            </Button>
          </div>
        }
        contentClassName="p-0"
      >
        <LocalRoutingBoundaryFields
          disabled={boundaryDisabled}
          draft={draft}
          errors={fieldErrors}
          groupOptions={groupOptions}
          onAllowDepletedFallbackChange={updateAllowDepletedFallback}
          onGroupPresetChange={updateRoutingGroupPreset}
          onLowBalanceThresholdCnyChange={updateLowBalanceThresholdCny}
          onMaxRateLimitEnabledChange={updateMaxRateLimitEnabled}
          onMaxRateMultiplierChange={updateMaxRateMultiplier}
          onBooleanChange={updateBooleanField}
          onNumericChange={updateBoundaryNumericField}
        />
        <div className="border-t border-border bg-slate-50 px-3 py-2 text-xs text-muted-foreground">
          当前预览：{previewEligibleCandidateCount} / {candidateCount} 个候选可参与路由。
        </div>
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
