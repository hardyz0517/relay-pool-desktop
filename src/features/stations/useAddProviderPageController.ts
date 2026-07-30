import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useToast } from "@/components/ui";
import { collectStationTask, startManualAuthorization, testStationLoginInput } from "@/lib/api/collector";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import {
  getCommonLoginPassword,
  getSettings,
  listCommonLoginOptions,
} from "@/lib/api/settings";
import {
  bindRemoteStationKey,
  createLocalStationKeyFromRemote,
  createRemoteStationKey,
  deleteRemoteStationKey,
  deleteStationKey,
  getRemoteKeyCapability,
  getStationCredentials,
  listRemoteStationKeys,
  listStationKeys,
  scanRemoteStationKeys,
  unbindRemoteStationKey,
  updateStationCredentials,
  updateStationKey,
} from "@/lib/api/stationKeys";
import { listStations, updateStation } from "@/lib/api/stations";
import {
  collectProviderDraftPreview,
  commitProviderDraft,
  createOrResumeProviderDraft,
  discardProviderDraft,
  patchProviderDraft,
  scanProviderDraftRemoteKeys,
  startProviderDraftAuthorization,
} from "@/lib/api/providerDrafts";
import { readError } from "@/lib/errors";
import { effectiveRateMultiplierForCredit } from "@/lib/formatters";
import { normalizeStationGroupOptions } from "@/lib/groupOptionViewModels";
import { queryKeys } from "@/lib/query/queryKeys";
import type { RemoteKeyCapability, RemoteStationKey, StationKey } from "@/lib/types/stationKeys";
import type { StationType } from "@/lib/types/stations";
import type { CommonLoginOptions } from "@/lib/types/settings";
import type { ProviderDraft } from "@/lib/types/providerDrafts";
import {
  createEmptyStationGroupDraft,
  type StationGroupDraft,
} from "./components/StationGroupRowsEditor";
import {
  createEmptyStationKeyDraft,
  type StationKeyDraft,
  type StationKeyGroupOption,
} from "./components/StationKeyRowsEditor";
import { providerPresets, type ProviderPresetId } from "./providerPresets";
import {
  createDefaultProviderForm,
  defaultPreset,
  draftRemoteCapability,
  formFromStation,
  getPresetDefaultStationName,
  serializeProviderDraft,
  type AddProviderFormState,
  type ConnectionTestState,
  type RemoteCreateInput,
} from "./pages/add-provider/formModel";
import {
  collectRemoteGroupOptions,
  dedupeGroupRows,
  groupBindingsToCurrentOptions,
  groupBindingsToDrafts,
  groupDraftToOption,
  groupsMatch,
  keyToDraft,
  isRemoteCreatedLocalKey,
  mergeGroupRowsWithSavedOptions,
  mergeKeyRowsWithSavedGroupOptions,
  mergeRemoteGroupOptions,
  normalizeCollectionIntervalMinutes,
  parseCreditPerCny,
  remoteKeyDisplayName,
  resolveRemoteCreatedLocalKeyIds,
  stationKeyToUpdateInput,
  syncRowsWithGroupRateOptions,
  validateGroupRows,
  validateKeyRows,
} from "./pages/add-provider/keyGroupModel";
import { saveGroupRows, saveKeyRows } from "./pages/add-provider/saveController";
import {
  editorFromProviderDraft,
  mergeProviderDraftPreviewGroups,
  providerDraftPayloadFromEditor,
} from "./pages/add-provider/providerDraftModel";

export type AddProviderPageControllerOptions = {
  stationId?: string | null;
  onBack: () => void;
  onCreated?: () => void;
  onUpdated?: () => void;
};

export function useAddProviderPageController({
  stationId,
  onBack,
  onCreated,
  onUpdated,
}: AddProviderPageControllerOptions) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const editing = Boolean(stationId);
  const [activeStationId, setActiveStationId] = useState<string | null>(stationId ?? null);
  const [providerDraftId, setProviderDraftId] = useState<string | null>(null);
  const providerDraftRef = useRef<ProviderDraft | null>(null);
  const providerDraftWriteQueueRef = useRef<Promise<void>>(Promise.resolve());
  const lastFlushedDraftSnapshotRef = useRef<string | null>(null);
  const lastFlushedSecretsRef = useRef({
    stationApiKey: "",
    loginPassword: "",
    keyApiKeys: {} as Record<string, string>,
  });
  const [form, setForm] = useState<AddProviderFormState>(createDefaultProviderForm);
  const [loading, setLoading] = useState(Boolean(stationId));
  const [saving, setSaving] = useState(false);
  const [testingConnection, setTestingConnection] = useState(false);
  const [startingAuthorization, setStartingAuthorization] = useState(false);
  const [connectionTest, setConnectionTest] = useState<ConnectionTestState>({
    status: "idle",
    message: null,
  });
  const [groupRows, setGroupRows] = useState<StationGroupDraft[]>([]);
  const [currentGroupOptions, setCurrentGroupOptions] = useState<StationKeyGroupOption[]>([]);
  const [keyRows, setKeyRows] = useState<StationKeyDraft[]>([createEmptyStationKeyDraft(0)]);
  const [remoteCapability, setRemoteCapability] = useState<RemoteKeyCapability | null>(
    stationId ? null : draftRemoteCapability(defaultPreset.stationType),
  );
  const [remoteCapabilityError, setRemoteCapabilityError] = useState<string | null>(null);
  const [remoteListError, setRemoteListError] = useState<string | null>(null);
  const [remoteKeys, setRemoteKeys] = useState<RemoteStationKey[]>([]);
  const [localStationKeys, setLocalStationKeys] = useState<StationKey[]>([]);
  const [remoteCreatedLocalKeyIds, setRemoteCreatedLocalKeyIds] = useState<Record<string, string>>({});
  const [remoteLoading, setRemoteLoading] = useState(false);
  const [createRemoteOpen, setCreateRemoteOpen] = useState(false);
  const [remoteKeyPendingDelete, setRemoteKeyPendingDelete] = useState<RemoteStationKey | null>(null);
  const [importedLocalKeyPendingDelete, setImportedLocalKeyPendingDelete] = useState<{
    remoteKey: RemoteStationKey;
    stationKeyId: string;
  } | null>(null);
  const [developerModeEnabled, setDeveloperModeEnabled] = useState(false);
  const [commonLoginOptions, setCommonLoginOptions] = useState<CommonLoginOptions>({
    emails: [],
    passwords: [],
  });
  const [passwordProfileLoading, setPasswordProfileLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [initialDraftSnapshot, setInitialDraftSnapshot] = useState(() =>
    serializeProviderDraft(createDefaultProviderForm(), [], [createEmptyStationKeyDraft(0)]),
  );
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const currentCreditPerCny = useMemo(() => parseCreditPerCny(form.creditPerCny), [form.creditPerCny]);
  const hasUnsavedChanges = serializeProviderDraft(form, groupRows, keyRows) !== initialDraftSnapshot;

  async function invalidateProviderWorkspaceCaches() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.stations }),
      queryClient.invalidateQueries({ queryKey: queryKeys.keyPool }),
    ]);
  }

  async function flushProviderDraft() {
    if (editing) {
      throw new Error("编辑已有供应商不使用新建草稿");
    }
    const snapshot = serializeProviderDraft(form, groupRows, keyRows);
    const payload = providerDraftPayloadFromEditor(form, groupRows, keyRows);
    const execute = async () => {
      let draft = providerDraftRef.current;
      if (!draft) {
        draft = await createOrResumeProviderDraft(payload);
        providerDraftRef.current = draft;
        setProviderDraftId(draft.id);
      }
      if (lastFlushedDraftSnapshotRef.current === snapshot) {
        return draft;
      }

      const previousSecrets = lastFlushedSecretsRef.current;
      const currentKeySecrets = Object.fromEntries(
        keyRows
          .filter((row) => !row.deleteRequested && row.apiKey.trim())
          .map((row) => [row.clientId, row.apiKey]),
      );
      const keySecretClientIds = new Set([
        ...Object.keys(previousSecrets.keyApiKeys),
        ...Object.keys(currentKeySecrets),
      ]);
      const patched = await patchProviderDraft({
        draftId: draft.id,
        expectedRevision: draft.revision,
        payload,
        stationApiKey: form.apiKey.trim()
          ? form.apiKey
          : previousSecrets.stationApiKey
            ? ""
            : null,
        loginPassword: form.loginPassword.trim()
          ? form.loginPassword
          : previousSecrets.loginPassword
            ? ""
            : null,
        keyApiKeys: [...keySecretClientIds].map((clientId) => ({
          clientId,
          apiKey: currentKeySecrets[clientId] ?? "",
        })),
      });
      providerDraftRef.current = patched;
      setProviderDraftId(patched.id);
      lastFlushedDraftSnapshotRef.current = snapshot;
      lastFlushedSecretsRef.current = {
        stationApiKey: form.apiKey,
        loginPassword: form.loginPassword,
        keyApiKeys: currentKeySecrets,
      };
      return patched;
    };

    const pending = providerDraftWriteQueueRef.current.then(execute, execute);
    providerDraftWriteQueueRef.current = pending.then(
      () => undefined,
      () => undefined,
    );
    return pending;
  }

  async function discardCurrentProviderDraft() {
    await providerDraftWriteQueueRef.current;
    const draft = providerDraftRef.current;
    if (draft?.state === "active") {
      await discardProviderDraft(draft.id);
    }
    providerDraftRef.current = null;
    setProviderDraftId(null);
  }

  const editableGroupOptions = useMemo(() => {
    const deletedCurrentGroups = currentGroupOptions.filter((option) =>
      groupRows.some((row) => row.deleteRequested && groupsMatch(row, option)),
    );
    return normalizeStationGroupOptions([
      ...currentGroupOptions.filter((option) => !deletedCurrentGroups.includes(option)),
      ...groupRows.flatMap((row) => {
        const option = groupDraftToOption(row, currentCreditPerCny);
        return option ? [option] : [];
      }),
    ]);
  }, [currentCreditPerCny, currentGroupOptions, groupRows]);

  const remoteGroupOptions = useMemo(
    () => mergeRemoteGroupOptions(editableGroupOptions, collectRemoteGroupOptions(remoteKeys, currentCreditPerCny)),
    [currentCreditPerCny, editableGroupOptions, remoteKeys],
  );

  const remoteUnsupportedReason = remoteCapability?.unsupportedReason ?? null;
  const remoteCapabilityUnavailableReason = remoteCapabilityError
    ? `远端 Key 能力读取失败：${remoteCapabilityError}`
    : remoteUnsupportedReason;
  const remoteActionUnavailableReason = remoteCapabilityUnavailableReason;
  const remoteDiscoveryReason =
    remoteCapabilityUnavailableReason ??
    (remoteListError ? `远端 Key 列表读取失败：${remoteListError}` : null);
  const scanRemoteDisabled =
    remoteLoading ||
    Boolean(remoteCapabilityError) ||
    (!activeStationId && !providerDraftId) ||
    remoteCapability?.canListRemoteKeys !== true;
  const savedStationCreateRemoteUnavailable = activeStationId
    ? remoteCapability?.canCreateRemoteKey !== true
    : false;
  const createRemoteDisabled =
    remoteLoading ||
    Boolean(remoteCapabilityError) ||
    !activeStationId ||
    savedStationCreateRemoteUnavailable;

  useEffect(() => {
    let alive = true;
    void getSettings()
      .then((settings) => {
        if (alive) {
          setDeveloperModeEnabled(settings.developerModeEnabled);
        }
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    let alive = true;
    void listCommonLoginOptions()
      .then((options) => {
        if (alive) setCommonLoginOptions(options);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    let alive = true;
    setActiveStationId(stationId ?? null);
    if (!stationId) {
      const nextForm = createDefaultProviderForm();
      const nextKeyRows = [createEmptyStationKeyDraft(0)];
      setForm(nextForm);
      setGroupRows([]);
      setCurrentGroupOptions([]);
      setKeyRows(nextKeyRows);
      setLocalStationKeys([]);
      setRemoteCreatedLocalKeyIds({});
      setRemoteCapability(draftRemoteCapability(defaultPreset.stationType));
      setRemoteCapabilityError(null);
      setRemoteListError(null);
      setRemoteKeys([]);
      setCreateRemoteOpen(false);
      setRemoteKeyPendingDelete(null);
      setProviderDraftId(null);
      providerDraftRef.current = null;
      lastFlushedDraftSnapshotRef.current = null;
      lastFlushedSecretsRef.current = {
        stationApiKey: "",
        loginPassword: "",
        keyApiKeys: {},
      };
      setLoading(true);
      setError(null);
      void createOrResumeProviderDraft(
        providerDraftPayloadFromEditor(nextForm, [], nextKeyRows),
      )
        .then((draft) => {
          if (!alive) return;
          const editor = editorFromProviderDraft(draft);
          const restoredKeyRows = editor.keyRows.length ? editor.keyRows : nextKeyRows;
          const snapshot = serializeProviderDraft(editor.form, editor.groupRows, restoredKeyRows);
          providerDraftRef.current = draft;
          setProviderDraftId(draft.id);
          setForm(editor.form);
          setGroupRows(editor.groupRows);
          setKeyRows(restoredKeyRows);
          setRemoteCapability(draftRemoteCapability(editor.form.stationType));
          setInitialDraftSnapshot(snapshot);
          lastFlushedDraftSnapshotRef.current = snapshot;
        })
        .catch((requestError) => {
          if (!alive) return;
          const message = readError(requestError);
          setError(message);
          toast.error("恢复供应商草稿失败", message);
        })
        .finally(() => {
          if (alive) setLoading(false);
        });
      return () => {
        alive = false;
      };
    }

    setProviderDraftId(null);
    providerDraftRef.current = null;
    setLoading(true);
    setError(null);
    void Promise.all([
      listStations(),
      getStationCredentials(stationId),
      listStationKeys(stationId),
      listStationGroupBindings(stationId),
      listGroupRateRecords(stationId),
      getRemoteKeyCapability(stationId)
        .then((capability) => ({ capability, error: null }))
        .catch((requestError) => ({ capability: null, error: readError(requestError) })),
      listRemoteStationKeys(stationId)
        .then((keys) => ({ keys, error: null }))
        .catch((requestError) => ({ keys: [], error: readError(requestError) })),
    ])
      .then(([stations, credentials, keys, groupBindings, groupRates, capabilityResult, discoveredRemoteKeysResult]) => {
        if (!alive) {
          return;
        }
        const station = stations.find((item) => item.id === stationId);
        if (!station) {
          throw new Error("未找到要编辑的供应商");
        }
        const nextForm = formFromStation(station, credentials);
        const nextGroupRows = dedupeGroupRows(groupBindingsToDrafts(groupBindings, groupRates));
        const nextKeyRows = keys.length ? keys.map(keyToDraft) : [];
        setForm(nextForm);
        setLocalStationKeys(keys);
        setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(discoveredRemoteKeysResult.keys, keys));
        setCurrentGroupOptions(groupBindingsToCurrentOptions(groupBindings, groupRates, station.creditPerCny));
        setGroupRows(nextGroupRows);
        setKeyRows(nextKeyRows);
        setRemoteCapability(capabilityResult.capability);
        setRemoteCapabilityError(capabilityResult.error);
        setRemoteListError(discoveredRemoteKeysResult.error);
        setRemoteKeys(discoveredRemoteKeysResult.keys);
        setConnectionTest({ status: "idle", message: null });
        setInitialDraftSnapshot(serializeProviderDraft(nextForm, nextGroupRows, nextKeyRows));
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

  useEffect(() => {
    setKeyRows((currentRows) => syncRowsWithGroupRateOptions(currentRows, editableGroupOptions));
  }, [editableGroupOptions]);

  useEffect(() => {
    if (editing || loading || !providerDraftId) return;
    const timeout = window.setTimeout(() => {
      void flushProviderDraft().catch((requestError) => {
        setError(readError(requestError));
      });
    }, 500);
    return () => window.clearTimeout(timeout);
  }, [editing, form, groupRows, keyRows, loading, providerDraftId]);

  function applyPreset(presetId: ProviderPresetId) {
    const preset = providerPresets.find((item) => item.id === presetId) ?? defaultPreset;
    setForm((current) => ({
      ...current,
      presetId: preset.id,
      name: getPresetDefaultStationName(preset),
      stationType: preset.stationType,
      websiteUrl: preset.websiteUrl,
      apiBaseUrl: preset.apiBaseUrl,
    }));
    if (!activeStationId) {
      setRemoteCapability(draftRemoteCapability(preset.stationType));
      setRemoteCapabilityError(null);
      setRemoteListError(null);
    }
    setError(null);
    setConnectionTest({ status: "idle", message: null });
  }

  async function refreshLocalStationKeyState(targetStationId: string) {
    const keys = await listStationKeys(targetStationId);
    setLocalStationKeys(keys);
    setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(remoteKeys, keys));
    setKeyRows(keys.length ? keys.map(keyToDraft) : []);
    return keys;
  }

  async function ensureStationForRemoteKeyActions() {
    if (activeStationId) {
      return activeStationId;
    }
    throw new Error("请先保存供应商，再使用远端同步功能");
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!form.name.trim()) {
      toast.info("请填写供应商名称");
      return;
    }
    if (!form.websiteUrl.trim()) {
      toast.info("请填写前端网址");
      return;
    }
    if (!form.apiBaseUrl.trim()) {
      toast.info("请填写 API Base URL");
      return;
    }

    try {
      validateGroupRows(groupRows);
      validateKeyRows(keyRows);
    } catch (validationError) {
      toast.info(readError(validationError));
      return;
    }

    setSaving(true);
    setError(null);
    try {
      if (activeStationId) {
        await updateStation({
          id: activeStationId,
          name: form.name.trim(),
          stationType: form.stationType,
          websiteUrl: form.websiteUrl.trim(),
          apiBaseUrl: form.apiBaseUrl.trim(),
          apiKey: form.apiKey.trim() ? form.apiKey.trim() : null,
          collectorProxyMode: form.collectorProxyMode,
          collectorProxyUrl: form.collectorProxyMode === "manual" && form.collectorProxyUrl.trim()
            ? form.collectorProxyUrl.trim()
            : null,
          enabled: form.enabled,
          creditPerCny: Number(form.creditPerCny),
          lowBalanceThresholdCny: form.lowBalanceThresholdCny.trim()
            ? Number(form.lowBalanceThresholdCny)
            : null,
          collectionIntervalMinutes: normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes),
          note: form.note.trim() ? form.note.trim() : null,
        });
        const savedGroupOptions = await saveGroupRows(activeStationId, groupRows, currentCreditPerCny);
        const rowsToSave = mergeKeyRowsWithSavedGroupOptions(keyRows, savedGroupOptions);
        setGroupRows((currentRows) => mergeGroupRowsWithSavedOptions(currentRows, savedGroupOptions));
        setKeyRows(rowsToSave);
        await saveKeyRows(activeStationId, rowsToSave);
        await refreshLocalStationKeyState(activeStationId);
        await invalidateProviderWorkspaceCaches();
        if (form.loginUsername.trim() || form.loginPassword.trim() || form.rememberPassword) {
          await updateStationCredentials({
            stationId: activeStationId,
            loginUsername: form.loginUsername.trim() ? form.loginUsername.trim() : null,
            loginPassword: form.loginPassword.trim() ? form.loginPassword : null,
            rememberPassword: form.rememberPassword,
          });
        }
        toast.success(editing ? "供应商已更新" : "供应商已添加");
        if (editing) {
          onUpdated?.();
        } else {
          onCreated?.();
        }
        return;
      }

      const draft = await flushProviderDraft();
      const commitKey = globalThis.crypto?.randomUUID?.() ?? `provider-draft-${Date.now()}`;
      const station = await commitProviderDraft(draft.id, draft.revision, commitKey);
      providerDraftRef.current = null;
      setProviderDraftId(null);
      setActiveStationId(station.id);
      await invalidateProviderWorkspaceCaches();
      toast.success("供应商已添加");
      onCreated?.();
    } catch (requestError) {
      const message = requestError instanceof Error ? requestError.message : String(requestError);
      setError(message);
      toast.error(editing ? "保存供应商失败" : "添加供应商失败", message);
    } finally {
      setSaving(false);
    }
  }

  async function handleTestConnection() {
    if (!form.websiteUrl.trim()) {
      toast.info("请填写前端网址");
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
      if (!editing) await flushProviderDraft();
      const result = await testStationLoginInput({
        stationType: form.stationType,
        websiteUrl: form.websiteUrl.trim(),
        loginUsername: form.loginUsername.trim(),
        loginPassword: form.loginPassword,
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

  async function handleStartManualAuthorization() {
    setStartingAuthorization(true);
    setError(null);
    try {
      if (activeStationId) {
        await startManualAuthorization(activeStationId);
      } else {
        const draft = await flushProviderDraft();
        await startProviderDraftAuthorization(draft.id);
      }
      toast.success("已打开网页登录授权窗口", "请在弹窗中完成登录，授权成功后会自动写回会话。");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("打开网页登录授权失败", message);
    } finally {
      setStartingAuthorization(false);
    }
  }

  async function handleScanRemoteKeys() {
    setRemoteLoading(true);
    setError(null);
    setRemoteListError(null);
    try {
      const result = activeStationId
        ? await scanRemoteStationKeys(activeStationId)
        : await scanProviderDraftRemoteKeys((await flushProviderDraft()).id);
      setRemoteCapability(result.capability);
      setRemoteCapabilityError(null);
      setRemoteKeys(result.keys);
      if (activeStationId) {
        const keys = await refreshLocalStationKeyState(activeStationId);
        setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(result.keys, keys));
      } else {
        setRemoteCreatedLocalKeyIds({});
      }
      toast.success("远端 Key 已更新", result.message || `发现 ${result.keys.length} 个远端 Key`);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      setRemoteListError(message);
      toast.error("获取远端 Key 失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleSyncRemoteGroups() {
    setRemoteLoading(true);
    setError(null);
    setRemoteListError(null);
    try {
      if (!activeStationId) {
        const draft = await flushProviderDraft();
        const preview = await collectProviderDraftPreview(draft.id, "groups");
        const syncedGroupRows = mergeProviderDraftPreviewGroups(groupRows, preview.groups);
        setGroupRows(syncedGroupRows);
        setRemoteCapability(draftRemoteCapability(form.stationType));
        setRemoteCapabilityError(null);
        toast.success("远端分组已同步到草稿", `发现 ${preview.groups.length} 个分组，保存后才会正式创建`);
        return;
      }
      const targetStationId = activeStationId;
      await collectStationTask(targetStationId, "groups");
      const [groupBindings, groupRates, capability] = await Promise.all([
        listStationGroupBindings(targetStationId),
        listGroupRateRecords(targetStationId),
        getRemoteKeyCapability(targetStationId).catch(() => null),
      ]);
      const syncedGroupRows = dedupeGroupRows(groupBindingsToDrafts(groupBindings, groupRates));
      setCurrentGroupOptions(groupBindingsToCurrentOptions(groupBindings, groupRates, currentCreditPerCny));
      setRemoteCapability(capability);
      setRemoteCapabilityError(null);
      setGroupRows(syncedGroupRows);
      toast.success("远端分组已同步", `发现 ${syncedGroupRows.length} 个分组，已用远端采集结果覆盖本地编辑区`);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      setRemoteListError(message);
      toast.error("同步远端分组失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleOpenCreateRemoteKey() {
    if (activeStationId) {
      setCreateRemoteOpen(true);
      return;
    }

    toast.info("草稿不支持修改远端数据，请先保存供应商");
  }

  function handleCreateRemoteKey(input: RemoteCreateInput) {
    void submitCreateRemoteKey(input);
  }

  async function submitCreateRemoteKey(input: RemoteCreateInput) {
    setRemoteLoading(true);
    setError(null);
    setRemoteListError(null);
    try {
      const targetStationId = await ensureStationForRemoteKeyActions();
      const result = await createRemoteStationKey({
        stationId: targetStationId,
        ...input,
      });
      setRemoteKeys((current) => [
        result.remoteKey,
        ...current.filter(
          (key) =>
            key.id !== result.remoteKey.id &&
            key.remoteKeyIdHash !== result.remoteKey.remoteKeyIdHash,
        ),
      ]);
      await refreshLocalStationKeyState(targetStationId);
      await invalidateProviderWorkspaceCaches();
      setCreateRemoteOpen(false);
      toast.success("远端 Key 已创建", result.message || "已同步保存为本地 Key");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("创建远端 Key 失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleBindRemoteKey(remoteKeyId: string, stationKeyId: string) {
    if (!activeStationId) {
      toast.info("草稿不支持修改远端或正式 Key 绑定");
      return;
    }
    setRemoteLoading(true);
    setError(null);
    try {
      const keys = await bindRemoteStationKey(remoteKeyId, stationKeyId);
      const nextRemoteKeys = keys.filter((key) => key.stationId === activeStationId);
      setRemoteKeys(nextRemoteKeys);
      setRemoteCreatedLocalKeyIds(
        resolveRemoteCreatedLocalKeyIds(nextRemoteKeys, localStationKeys),
      );
      toast.success("远端 Key 已绑定");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("绑定远端 Key 失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleImportRemoteKey(remoteKey: RemoteStationKey) {
    if (!activeStationId) {
      toast.info("草稿阶段只能查看远端 Key");
      return;
    }
    setRemoteLoading(true);
    setError(null);
    try {
      await createLocalKeyFromRemote(remoteKey);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("导入本地 Key 失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleUnbindRemoteKey(remoteKey: RemoteStationKey) {
    if (!activeStationId || remoteLoading) {
      return;
    }
    setRemoteLoading(true);
    setError(null);
    try {
      const nextRemoteKeys = (await unbindRemoteStationKey(remoteKey.id, activeStationId)).filter(
        (key) => key.stationId === activeStationId,
      );
      setRemoteKeys(nextRemoteKeys);
      setRemoteCreatedLocalKeyIds(
        resolveRemoteCreatedLocalKeyIds(nextRemoteKeys, localStationKeys),
      );
      await invalidateProviderWorkspaceCaches();
      toast.success("本地关联已解除", "Key 池中的本地 Key 保持不变。");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("解除本地关联失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  function requestDeleteImportedLocalKey(remoteKey: RemoteStationKey) {
    const stationKeyId = remoteCreatedLocalKeyIds[remoteKey.id];
    if (!stationKeyId || remoteLoading) {
      return;
    }
    setImportedLocalKeyPendingDelete({ remoteKey, stationKeyId });
  }

  function cancelDeleteImportedLocalKey() {
    if (!remoteLoading) {
      setImportedLocalKeyPendingDelete(null);
    }
  }

  async function confirmDeleteImportedLocalKey() {
    const pending = importedLocalKeyPendingDelete;
    if (!pending || remoteLoading) {
      return;
    }
    setRemoteLoading(true);
    setError(null);
    try {
      await deleteRemoteCreatedLocalKey(pending.remoteKey, pending.stationKeyId);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("删除导入的本地 Key 失败", message);
    } finally {
      setImportedLocalKeyPendingDelete(null);
      setRemoteLoading(false);
    }
  }

  async function createLocalKeyFromRemote(remoteKey: RemoteStationKey) {
    const targetStationId = await ensureStationForRemoteKeyActions();
    const result = await createLocalStationKeyFromRemote(remoteKey.id, targetStationId);
    await updateStationKey(stationKeyToUpdateInput(result.stationKey, {
      rateMultiplier: effectiveRateMultiplierForCredit(remoteKey.rateMultiplier, currentCreditPerCny),
    }));
    const nextRemoteKeys = (await bindRemoteStationKey(remoteKey.id, result.stationKey.id)).filter(
      (key) => key.stationId === targetStationId,
    );
    const nextLocalKeys = await refreshLocalStationKeyState(targetStationId);
    await invalidateProviderWorkspaceCaches();
    setRemoteKeys(nextRemoteKeys);
    setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(nextRemoteKeys, nextLocalKeys));
    toast.success("已创建本地 Key", result.message || `${remoteKeyDisplayName(remoteKey)} 已保存为本地 Key。`);
  }

  async function deleteRemoteCreatedLocalKey(
    remoteKey: RemoteStationKey,
    expectedStationKeyId: string,
  ) {
    const expectedLocalKey = localStationKeys.find((key) => key.id === expectedStationKeyId);
    if (!expectedLocalKey || !isRemoteCreatedLocalKey(remoteKey, expectedLocalKey)) {
      throw new Error("这把本地 Key 不是由远端导入的，未删除。");
    }

    await deleteStationKey(expectedStationKeyId);
    const [nextRemoteKeys, nextLocalKeys] = await Promise.all([
      listRemoteStationKeys(remoteKey.stationId),
      refreshLocalStationKeyState(remoteKey.stationId),
    ]);
    await invalidateProviderWorkspaceCaches();
    setRemoteKeys(nextRemoteKeys);
    setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(nextRemoteKeys, nextLocalKeys));
    toast.success("已删除导入的本地 Key");
  }

  function requestDeleteRemoteKey(remoteKey: RemoteStationKey) {
    if (!activeStationId || remoteLoading || remoteCapability?.canDeleteRemoteKeys !== true) {
      return;
    }
    setRemoteKeyPendingDelete(remoteKey);
  }

  function cancelDeleteRemoteKey() {
    if (!remoteLoading) {
      setRemoteKeyPendingDelete(null);
    }
  }

  async function confirmDeleteRemoteKey() {
    const remoteKey = remoteKeyPendingDelete;
    if (!remoteKey || remoteLoading) {
      return;
    }

    setRemoteLoading(true);
    setError(null);
    setRemoteListError(null);
    try {
      const result = await deleteRemoteStationKey(remoteKey.id, remoteKey.stationId);
      setRemoteKeys(result.keys);
      setRemoteCreatedLocalKeyIds(resolveRemoteCreatedLocalKeyIds(result.keys, localStationKeys));
      await invalidateProviderWorkspaceCaches();
      toast.success(
        result.alreadyAbsent ? "远端 Key 已不存在" : "远端 Key 已删除",
        result.message || `${remoteKeyDisplayName(remoteKey)} 已从远端删除，本地 Key 保留。`,
      );
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      setRemoteListError(message);
      toast.error("删除远端 Key 失败", message);
    } finally {
      setRemoteKeyPendingDelete(null);
      setRemoteLoading(false);
    }
  }

  function handleAddLocalKey() {
    setKeyRows((currentRows) => [
      ...currentRows,
      createEmptyStationKeyDraft(currentRows.length),
    ]);
  }

  function handleCopyWebsiteUrl() {
    setForm((current) => ({
      ...current,
      apiBaseUrl: current.websiteUrl,
    }));
  }

  function handleCommonEmailSelect(profileId: string) {
    const email = commonLoginOptions.emails.find((item) => item.id === profileId);
    if (!email) return;
    setForm((current) => ({ ...current, loginUsername: email.email }));
    resetConnectionTest();
  }

  async function handleCommonPasswordSelect(profileId: string) {
    setPasswordProfileLoading(true);
    try {
      const password = await getCommonLoginPassword(profileId);
      setForm((current) => ({
        ...current,
        loginPassword: password,
        rememberPassword: true,
      }));
      resetConnectionTest();
    } catch (requestError) {
      toast.error("填充常用密码失败", readError(requestError));
    } finally {
      setPasswordProfileLoading(false);
    }
  }

  function handleStationTypeChange(stationType: StationType) {
    setForm({ ...form, stationType });
    if (!activeStationId) {
      setRemoteCapability(draftRemoteCapability(stationType));
      setRemoteCapabilityError(null);
      setRemoteListError(null);
    }
  }

  function handleAddGroup() {
    setGroupRows((currentRows) =>
      dedupeGroupRows([
        ...currentRows,
        createEmptyStationGroupDraft(currentRows.length),
      ]),
    );
  }

  function handleGroupRowsChange(rows: StationGroupDraft[]) {
    setGroupRows(dedupeGroupRows(rows));
  }

  function resetConnectionTest() {
    setConnectionTest({ status: "idle", message: null });
  }

  function closeCreateRemoteDialog() {
    setCreateRemoteOpen(false);
  }

  function closeDiscardConfirm() {
    setDiscardConfirmOpen(false);
  }

  async function exitAndDiscardDraft() {
    try {
      if (!editing) await discardCurrentProviderDraft();
      onBack();
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("丢弃供应商草稿失败", message);
    }
  }

  function confirmDiscardChanges() {
    setDiscardConfirmOpen(false);
    void exitAndDiscardDraft();
  }

  function requestExit() {
    if (hasUnsavedChanges) {
      setDiscardConfirmOpen(true);
      return;
    }
    void exitAndDiscardDraft();
  }

  return {
    activeStationId,
    applyPreset,
    cancelDeleteImportedLocalKey,
    cancelDeleteRemoteKey,
    closeCreateRemoteDialog,
    closeDiscardConfirm,
    commonLoginOptions,
    confirmDiscardChanges,
    confirmDeleteImportedLocalKey,
    confirmDeleteRemoteKey,
    connectionTest,
    createRemoteDisabled,
    createRemoteOpen,
    currentCreditPerCny,
    developerModeEnabled,
    discardConfirmOpen,
    editableGroupOptions,
    editing,
    error,
    form,
    groupRows,
    handleAddGroup,
    handleAddLocalKey,
    handleBindRemoteKey,
    handleCommonEmailSelect,
    handleCommonPasswordSelect,
    handleCreateRemoteKey,
    handleGroupRowsChange,
    handleOpenCreateRemoteKey,
    handleImportRemoteKey,
    handleScanRemoteKeys,
    handleStartManualAuthorization,
    handleStationTypeChange,
    handleSubmit,
    handleSyncRemoteGroups,
    handleTestConnection,
    keyRows,
    importedLocalKeyPendingDelete,
    loading,
    passwordProfileLoading,
    providerDraftId,
    localStationKeys,
    remoteCapability,
    remoteCapabilityError,
    remoteCapabilityUnavailableReason: remoteActionUnavailableReason,
    remoteCreatedLocalKeyIds,
    remoteDiscoveryReason,
    remoteGroupOptions,
    remoteKeys,
    remoteKeyPendingDelete,
    remoteListError,
    remoteLoading,
    remoteUnsupportedReason,
    requestDeleteRemoteKey,
    requestDeleteImportedLocalKey,
    requestExit,
    resetConnectionTest,
    saving,
    scanRemoteDisabled,
    setForm,
    setKeyRows,
    testingConnection,
    startingAuthorization,
    handleCopyWebsiteUrl,
    handleUnbindRemoteKey,
  };
}
