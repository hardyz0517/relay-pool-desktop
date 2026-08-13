import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useToast } from "@/components/ui";
import { remoteKeyRefreshFailure } from "@/lib/collectorEvents";
import { collectStationTask, startManualAuthorization, testStationLoginInput } from "@/lib/api/collector";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import {
  getCommonLoginPassword,
  getSettings,
  listCommonLoginOptions,
} from "@/lib/api/settings";
import {
  createLocalStationKeyFromRemote,
  createRemoteStationKey,
  deleteRemoteStationKey,
  deleteStationKey,
  getRemoteKeyCapability,
  getStationCredentials,
  listRemoteStationKeys,
  listStationKeys,
  scanRemoteStationKeys,
  updateStationCredentials,
  updateStationKey,
} from "@/lib/api/stationKeys";
import { clearStationCapacityDomain, getStationCapacityDomain, listStations, updateStation, upsertStationCapacityDomain } from "@/lib/api/stations";
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
import { discoverCreatedStationKeyModels } from "@/lib/stationKeyModelDiscovery";
import type { RemoteKeyCapability, RemoteStationKey, StationKey } from "@/lib/types/stationKeys";
import type { StationCapacityDomain, StationType } from "@/lib/types/stations";
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
  collectNewlyDeletedPersistedKeyIds,
  dedupeGroupRows,
  deriveRemoteKeyEditorState,
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
  const [capacityDomain, setCapacityDomain] = useState<StationCapacityDomain | null>(null);
  const [capacityDomainSaving, setCapacityDomainSaving] = useState(false);
  const [initialDraftSnapshot, setInitialDraftSnapshot] = useState(() =>
    serializeProviderDraft(createDefaultProviderForm(), [], [createEmptyStationKeyDraft(0)]),
  );
  const [discardConfirmOpen, setDiscardConfirmOpen] = useState(false);
  const currentCreditPerCny = useMemo(() => parseCreditPerCny(form.creditPerCny), [form.creditPerCny]);
  const hasUnsavedChanges = serializeProviderDraft(form, groupRows, keyRows) !== initialDraftSnapshot;
  const remoteKeyEditorState = useMemo(
    () => deriveRemoteKeyEditorState(remoteKeys, localStationKeys, keyRows),
    [keyRows, localStationKeys, remoteKeys],
  );

  async function invalidateProviderWorkspaceCaches() {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.stations }),
      queryClient.invalidateQueries({ queryKey: queryKeys.keyPool }),
    ]);
  }

  async function autoDiscoverCreatedKeyModels(stationKeyIds: string[]) {
    const summary = await discoverCreatedStationKeyModels(stationKeyIds);
    if (summary.requestedCount === 0) {
      return;
    }
    if (summary.failures.length > 0) {
      toast.info(
        "本地密钥已创建，部分模型列表获取失败",
        `${summary.failures.length}/${summary.requestedCount} 把密钥获取失败：${readError(summary.failures[0].error)}`,
      );
      return;
    }
    if (summary.updatedCount === 0) {
      toast.info("本地密钥已创建，未获取到模型", "模型接口返回了空列表。");
      return;
    }
    toast.success(
      "模型列表已自动获取并保存",
      `${summary.updatedCount} 把密钥共获取 ${summary.modelCount} 个模型。`,
    );
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
    ? `远端密钥能力读取失败：${remoteCapabilityError}`
    : remoteUnsupportedReason;
  const remoteActionUnavailableReason = remoteCapabilityUnavailableReason;
  const remoteDiscoveryReason =
    remoteCapabilityUnavailableReason ??
    (remoteListError ? `远端密钥列表读取失败：${remoteListError}` : null);
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
      setRemoteCapability(draftRemoteCapability(defaultPreset.stationType));
      setRemoteCapabilityError(null);
      setRemoteListError(null);
      setRemoteKeys([]);
      setCapacityDomain(null);
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
      getStationCapacityDomain(stationId),
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
      .then(([stations, domain, credentials, keys, groupBindings, groupRates, capabilityResult, discoveredRemoteKeysResult]) => {
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
        setCapacityDomain(domain);
        setLocalStationKeys(keys);
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
    setKeyRows(keys.length ? keys.map(keyToDraft) : []);
    return keys;
  }

  async function refreshStationKeyState(targetStationId: string) {
    const [nextRemoteKeys, nextLocalKeys] = await Promise.all([
      listRemoteStationKeys(targetStationId),
      listStationKeys(targetStationId),
    ]);
    setRemoteKeys(nextRemoteKeys);
    setLocalStationKeys(nextLocalKeys);
    setKeyRows(nextLocalKeys.length ? nextLocalKeys.map(keyToDraft) : []);
    return { remoteKeys: nextRemoteKeys, localKeys: nextLocalKeys };
  }

  function handleKeyRowsChange(nextRows: StationKeyDraft[]) {
    const newlyDeletedPersistedKeyIds = collectNewlyDeletedPersistedKeyIds(keyRows, nextRows);
    setKeyRows(nextRows);

    if (!activeStationId || newlyDeletedPersistedKeyIds.length === 0) {
      return;
    }
    void deletePersistedLocalKeysImmediately(activeStationId, newlyDeletedPersistedKeyIds);
  }

  async function deletePersistedLocalKeysImmediately(
    targetStationId: string,
    stationKeyIds: string[],
  ) {
    setRemoteLoading(true);
    setError(null);
    try {
      await Promise.all(stationKeyIds.map((stationKeyId) => deleteStationKey(stationKeyId)));
      await refreshStationKeyState(targetStationId);
      await invalidateProviderWorkspaceCaches();
      toast.success(stationKeyIds.length === 1 ? "本地密钥已删除" : `已删除 ${stationKeyIds.length} 个本地密钥`);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      try {
        await refreshStationKeyState(targetStationId);
      } catch {
        // Keep the original mutation error as the actionable failure.
      }
      toast.error("删除本地密钥失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function ensureStationForRemoteKeyActions() {
    if (activeStationId) {
      return activeStationId;
    }
    throw new Error("请先保存供应商，再使用远端同步功能");
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (remoteLoading) {
      toast.info("请等待密钥操作完成");
      return;
    }
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
          lowBalanceThresholdCny: null,
          collectionIntervalMinutes: normalizeCollectionIntervalMinutes(form.collectionIntervalMinutes),
          note: form.note.trim() ? form.note.trim() : null,
        });
        const savedGroupOptions = await saveGroupRows(activeStationId, groupRows, currentCreditPerCny);
        const rowsToSave = mergeKeyRowsWithSavedGroupOptions(keyRows, savedGroupOptions);
        setGroupRows((currentRows) => mergeGroupRowsWithSavedOptions(currentRows, savedGroupOptions));
        setKeyRows(rowsToSave);
        const createdStationKeyIds = await saveKeyRows(activeStationId, rowsToSave);
        await autoDiscoverCreatedKeyModels(createdStationKeyIds);
        await refreshStationKeyState(activeStationId);
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
      try {
        const createdStationKeys = await listStationKeys(station.id);
        await autoDiscoverCreatedKeyModels(createdStationKeys.map((key) => key.id));
      } catch (modelDiscoveryError) {
        toast.info("供应商已创建，模型列表获取失败", readError(modelDiscoveryError));
      }
      providerDraftRef.current = null;
      setProviderDraftId(null);
      setActiveStationId(station.id);
      await invalidateProviderWorkspaceCaches();
      toast.success("供应商已添加");
      onCreated?.();
    } catch (requestError) {
      const message = readError(requestError);
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
    if (form.stationType === "sub2api") {
      setTestingConnection(true);
      setError(null);
      setConnectionTest({ status: "testing", message: "正在使用授权会话采集..." });
      try {
        if (activeStationId) {
          const result = await collectStationTask(activeStationId, "full");
          const status = result.snapshot.status === "success" ? "success" : "warning";
          setConnectionTest({
            status,
            message: result.snapshot.errorMessage ?? `采集${status === "success" ? "成功" : "已完成"}`,
          });
        } else {
          const draft = await flushProviderDraft();
          const preview = await collectProviderDraftPreview(draft.id, "full");
          const status = preview.status === "success" ? "success" : "warning";
          setConnectionTest({
            status,
            message: `授权会话采集${status === "success" ? "成功" : "已完成"}：${preview.groups.length} 个分组`,
          });
        }
      } catch (requestError) {
        const message = readError(requestError);
        setConnectionTest({ status: "error", message });
        toast.error("Sub2API 授权会话采集失败", message);
      } finally {
        setTestingConnection(false);
      }
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
        await refreshLocalStationKeyState(activeStationId);
      }
      toast.success("远端密钥已更新", result.message || `发现 ${result.keys.length} 个远端密钥`);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      setRemoteListError(message);
      toast.error("获取远端密钥失败", message);
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
      const collectionResult = await collectStationTask(targetStationId, "groups");
      const [groupBindings, groupRates, localKeys, remoteKeys, capability] = await Promise.all([
        listStationGroupBindings(targetStationId),
        listGroupRateRecords(targetStationId),
        listStationKeys(targetStationId),
        listRemoteStationKeys(targetStationId),
        getRemoteKeyCapability(targetStationId).catch(() => null),
      ]);
      const syncedGroupRows = dedupeGroupRows(groupBindingsToDrafts(groupBindings, groupRates));
      setCurrentGroupOptions(groupBindingsToCurrentOptions(groupBindings, groupRates, currentCreditPerCny));
      setLocalStationKeys(localKeys);
      setKeyRows(localKeys.length ? localKeys.map(keyToDraft) : []);
      setRemoteKeys(remoteKeys);
      setRemoteCapability(capability);
      setGroupRows(syncedGroupRows);
      const refreshFailure = remoteKeyRefreshFailure(collectionResult);
      setRemoteCapabilityError(null);
      setRemoteListError(refreshFailure?.message ?? null);
      if (refreshFailure) {
        toast.error("倍率已采集，但远端密钥刷新失败", refreshFailure.message);
      } else {
        toast.success("远端分组和密钥已同步", `发现 ${syncedGroupRows.length} 个分组、${remoteKeys.length} 把远端密钥`);
      }
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
      const createdLocalKey = !localStationKeys.some((key) => key.id === result.stationKey.id);
      if (createdLocalKey) {
        await autoDiscoverCreatedKeyModels([result.stationKey.id]);
      }
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
      toast.success("远端密钥已创建", result.message || "已同步保存为本地密钥");
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("创建远端密钥失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  async function handleImportRemoteKey(remoteKey: RemoteStationKey) {
    if (!activeStationId) {
      toast.info("草稿阶段只能查看远端密钥");
      return;
    }
    setRemoteLoading(true);
    setError(null);
    try {
      await createLocalKeyFromRemote(remoteKey);
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("导入本地密钥失败", message);
    } finally {
      setRemoteLoading(false);
    }
  }

  function requestDeleteImportedLocalKey(remoteKey: RemoteStationKey) {
    const stationKeyId = remoteKeyEditorState.localKeyIdsCreatedByRemote[remoteKey.id];
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
      toast.error("删除导入的本地密钥失败", message);
    } finally {
      setImportedLocalKeyPendingDelete(null);
      setRemoteLoading(false);
    }
  }

  async function createLocalKeyFromRemote(remoteKey: RemoteStationKey) {
    const targetStationId = await ensureStationForRemoteKeyActions();
    const result = await createLocalStationKeyFromRemote(remoteKey.id, targetStationId);
    const createdLocalKey = !localStationKeys.some((key) => key.id === result.stationKey.id);
    await updateStationKey(stationKeyToUpdateInput(result.stationKey, {
      rateMultiplier: effectiveRateMultiplierForCredit(remoteKey.rateMultiplier, currentCreditPerCny),
    }));
    if (createdLocalKey) {
      await autoDiscoverCreatedKeyModels([result.stationKey.id]);
    }
    await refreshStationKeyState(targetStationId);
    await invalidateProviderWorkspaceCaches();
    toast.success("已创建本地密钥", result.message || `${remoteKeyDisplayName(remoteKey)} 已保存为本地密钥。`);
  }

  async function deleteRemoteCreatedLocalKey(
    remoteKey: RemoteStationKey,
    expectedStationKeyId: string,
  ) {
    const expectedLocalKey = localStationKeys.find((key) => key.id === expectedStationKeyId);
    if (!expectedLocalKey || !isRemoteCreatedLocalKey(remoteKey, expectedLocalKey)) {
      throw new Error("这把本地密钥不是由远端导入的，未删除。");
    }

    await deleteStationKey(expectedStationKeyId);
    await refreshStationKeyState(remoteKey.stationId);
    await invalidateProviderWorkspaceCaches();
    toast.success("已删除导入的本地密钥");
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
      await invalidateProviderWorkspaceCaches();
      toast.success(
        result.alreadyAbsent ? "远端密钥已不存在" : "远端密钥已删除",
        result.message || `${remoteKeyDisplayName(remoteKey)} 已从远端删除，本地密钥保留。`,
      );
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      setRemoteListError(message);
      toast.error("删除远端密钥失败", message);
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

  async function handleSaveCapacityDomain(input: { providerFamily: string; deploymentIdentity: string; regionIdentity: string }) {
    if (!activeStationId) return;
    setCapacityDomainSaving(true);
    try {
      const saved = await upsertStationCapacityDomain({
        stationId: activeStationId,
        expectedRevision: capacityDomain?.revision ?? 0,
        providerFamily: input.providerFamily.trim(),
        deploymentIdentity: input.deploymentIdentity.trim() || null,
        regionIdentity: input.regionIdentity.trim() || null,
      });
      setCapacityDomain(saved);
      toast.success("容量域身份已保存");
    } catch (requestError) {
      toast.error("保存容量域身份失败", readError(requestError));
    } finally { setCapacityDomainSaving(false); }
  }

  async function handleClearCapacityDomain() {
    if (!activeStationId || !capacityDomain) return;
    setCapacityDomainSaving(true);
    try {
      await clearStationCapacityDomain(activeStationId, capacityDomain.revision);
      setCapacityDomain(null);
      toast.success("容量域身份已清除");
    } catch (requestError) {
      toast.error("清除容量域身份失败", readError(requestError));
    } finally { setCapacityDomainSaving(false); }
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
    capacityDomain,
    capacityDomainSaving,
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
    handleCommonEmailSelect,
    handleCommonPasswordSelect,
    handleCreateRemoteKey,
    handleGroupRowsChange,
    handleOpenCreateRemoteKey,
    handleImportRemoteKey,
    handleKeyRowsChange,
    handleScanRemoteKeys,
    handleStartManualAuthorization,
    handleStationTypeChange,
    handleSaveCapacityDomain,
    handleClearCapacityDomain,
    handleSubmit,
    handleSyncRemoteGroups,
    handleTestConnection,
    keyRows,
    importedLocalKeyPendingDelete,
    loading,
    passwordProfileLoading,
    pendingUnbindRemoteKeyIds: remoteKeyEditorState.pendingUnbindRemoteKeyIds,
    providerDraftId,
    localStationKeys: remoteKeyEditorState.localKeys,
    remoteCapability,
    remoteCapabilityError,
    remoteCapabilityUnavailableReason: remoteActionUnavailableReason,
    remoteCreatedLocalKeyIds: remoteKeyEditorState.localKeyIdsCreatedByRemote,
    remoteDiscoveryReason,
    remoteGroupOptions,
    remoteKeys: remoteKeyEditorState.remoteKeys,
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
    testingConnection,
    startingAuthorization,
    handleCopyWebsiteUrl,
  };
}
