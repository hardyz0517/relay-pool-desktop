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
  parseLocalRoutingSettingsDraft,
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
  const [saveState, setSaveState] = useState<SaveState>("idle");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [fieldErrors, setFieldErrors] = useState<LocalRoutingSettingsErrors>({});
  const loadOperationRef = useRef(0);
  const saveOperationRef = useRef(0);

  useEffect(() => {
    void loadCurrentSettings();
    return () => {
      loadOperationRef.current += 1;
      saveOperationRef.current += 1;
    };
  }, []);

  const dirty = useMemo(
    () => Boolean(draft && savedDraft && JSON.stringify(draft) !== JSON.stringify(savedDraft)),
    [draft, savedDraft],
  );
  const visibleSaveState: VisibleSaveState =
    saveState === "saving" || saveState === "error"
      ? saveState
      : dirty
        ? "dirty"
        : saveState;

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
      setSettings(nextSettings);
      setDraft(nextDraft);
      setSavedDraft(nextDraft);
      setFieldErrors({});
      setSaveError(null);
      setSaveState("idle");
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
    if (!settings || !draft || saveState === "saving") {
      return;
    }
    const parsed = parseLocalRoutingSettingsDraft(draft);
    if (!parsed.ok) {
      setFieldErrors(parsed.errors);
      setSaveState("error");
      setSaveError("请修正标记的参数");
      requestAnimationFrame(() => {
        document.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
      });
      return;
    }

    const operationId = saveOperationRef.current + 1;
    saveOperationRef.current = operationId;
    setSaveState("saving");
    setSaveError(null);
    setFieldErrors({});
    try {
      const nextSettings = await updateSettings({
        ...appSettingsToUpdateInput(settings),
        defaultRoutingStrategy: "automatic_balanced",
        maxRateMultiplier: parsed.value.maxRateMultiplier,
        defaultRoutingGroupFilter: parsed.value.defaultRoutingGroupFilter,
        schedulerAdvancedSettings: parsed.value.schedulerAdvancedSettings,
      });
      if (operationId !== saveOperationRef.current) {
        return;
      }
      const nextDraft = createLocalRoutingSettingsDraft(nextSettings);
      setSettings(nextSettings);
      setDraft(nextDraft);
      setSavedDraft(nextDraft);
      setSaveState("saved");
      window.dispatchEvent(new Event(SETTINGS_UPDATED_EVENT));
      toast.success("路由设置已保存");
    } catch (requestError) {
      if (operationId !== saveOperationRef.current) {
        return;
      }
      const message = readError(requestError);
      setSaveState("error");
      setSaveError(message);
      toast.error("保存路由设置失败", message);
    }
  }

  function updateDraft(update: (current: LocalRoutingSettingsDraft) => LocalRoutingSettingsDraft) {
    setDraft((current) => (current ? update(current) : current));
    setSaveState((current) => (current === "saving" ? current : "idle"));
    setSaveError(null);
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
  const disabled = loading || saveState === "saving";

  return (
    <form className="grid gap-3" onSubmit={(event) => void handleSubmit(event)}>
      <SectionCard title="路由边界" contentClassName="p-0">
        <LocalRoutingBoundaryFields
          disabled={disabled}
          draft={draft}
          errors={fieldErrors}
          groupOptions={groupOptions}
          onGroupPresetChange={(defaultRoutingGroupPreset) =>
            updateDraft((current) => ({ ...current, defaultRoutingGroupPreset }))
          }
          onMaxRateMultiplierChange={(maxRateMultiplier) => {
            clearFieldError("maxRateMultiplier");
            updateDraft((current) => ({ ...current, maxRateMultiplier }));
          }}
          onBooleanChange={updateBooleanField}
          onNumericChange={updateNumericField}
        />
      </SectionCard>

      <SectionCard
        title="自动调度参数"
        action={
          <div className="flex flex-wrap items-center justify-end gap-2">
            <StatusBadge tone={saveStateTones[visibleSaveState]}>
              {saveStateLabels[visibleSaveState]}
            </StatusBadge>
            <Button disabled={disabled} type="button" variant="outline" onClick={resetSchedulerDefaults}>
              <RotateCcw className="h-4 w-4" />
              恢复默认
            </Button>
            <Button disabled={disabled || !dirty} type="submit">
              <Save className="h-4 w-4" />
              保存设置
            </Button>
          </div>
        }
        contentClassName="p-0"
      >
        <LocalRoutingSchedulerFields
          draft={draft}
          disabled={disabled}
          errors={fieldErrors}
          onNumericChange={updateNumericField}
          onBooleanChange={updateBooleanField}
        />
        {saveError ? (
          <div className="border-t border-rose-100 bg-rose-50 px-4 py-2 text-xs text-rose-700">
            {saveError}
          </div>
        ) : null}
      </SectionCard>
    </form>
  );
}
