import { beforeEach, describe, expect, it, vi } from "vitest";

const transport = vi.hoisted(() => ({
  invoke: vi.fn(),
  invokeNonIdempotent: vi.fn(),
}));

vi.mock("@/lib/bridge/transport", () => transport);

import {
  bindRemoteStationKey,
  clearChangeEvents,
  clearRequestLogs,
  clearStationCredentials,
  collectStationInfo,
  collectStationTask,
  collectSub2apiStation,
  createChannelMonitor,
  createChannelMonitorTemplate,
  createLocalStationKeyFromRemote,
  createRemoteStationKey,
  createStation,
  createStationKey,
  deleteChannelMonitor,
  deleteChannelMonitorTemplate,
  deleteModelAlias,
  deletePricingRule,
  deleteStationKey,
  deleteStation,
  detectStationInfo,
  detectSub2apiStation,
  dismissChangeEvent,
  duplicateChannelMonitorTemplate,
  getRemoteKeyCapability,
  getLatestCollectorSnapshot,
  getProxyStatus,
  getSettings,
  getStationKeyCapabilities,
  getStationCredentials,
  getStationKeyHealth,
  listKeyPoolItems,
  listModelBasePrices,
  listChangeEvents,
  listChangeEventsForStation,
  listChannelMonitorRuns,
  listChannelMonitorSummaries,
  listChannelMonitorTemplates,
  listChannelMonitors,
  listChannelStatusSummaries,
  listBalanceSnapshots,
  listBalanceSnapshotsForStation,
  listCollectorRuns,
  listCollectorSnapshots,
  listCurrentStationBalanceSnapshots,
  listGroupRateRecords,
  loadChannelStatusWorkspace,
  loadPricingComparisonWorkspace,
  loadLocalRoutingWorkspace,
  listRemoteStationKeys,
  listRequestLogs,
  listModelAliases,
  listPricingRules,
  listStationEndpointHealth,
  listStationKeyHealth,
  listStationKeys,
  listStations,
  listStationGroupBindings,
  listStationGroupOptions,
  markChangeEventRead,
  markChangeEventsRead,
  reorderKeyPool,
  reorderStationKeys,
  reorderStations,
  resetModelBasePricesToBuiltins,
  resolveChangeEvent,
  resolveStationKeyPricingContext,
  runChannelMonitorNow,
  saveStationKeyWithDefaults,
  scanRemoteStationKeys,
  simulateRoute,
  testStationLogin,
  testStationLoginInput,
  unbindRemoteStationKey,
  updateChannelMonitor,
  updateChannelMonitorTemplate,
  updateStationCredentials,
  updateStationKey,
  updateStationKeyGroupBinding,
  updateStationSession,
  updateStationKeyCapabilities,
  updateSettings,
  updateStation,
  upsertBalanceSnapshot,
  upsertChangeEvent,
  upsertModelAlias,
  upsertModelBasePrice,
  upsertPricingRule,
  upsertStationGroupBinding,
  type CreateStationInputDto,
  type UpdateSettingsInputDto,
} from "./generated";

const stationInput: CreateStationInputDto = {
  name: "Smoke station",
  stationType: "openai-compatible",
  websiteUrl: "https://example.test",
  apiBaseUrl: "https://example.test/v1",
  apiKey: "sk-smoke-redacted",
  collectorProxyMode: "inherit",
  collectorProxyUrl: null,
  enabled: true,
  creditPerCny: 1,
  lowBalanceThresholdCny: null,
  collectionIntervalMinutes: 10,
  note: null,
};

const settingsInput = {
  localProxyPort: 8317,
  defaultRoutingStrategy: "automatic_balanced",
  collectorProxyMode: "direct",
  collectorProxyUrl: null,
  maxRateMultiplier: null,
  lowBalanceThresholdCny: 10,
  collectorIntervalMinutes: 5,
  balanceIntervalMinutes: 5,
  groupRateIntervalMinutes: 20,
  modelListIntervalMinutes: 60,
  pricingRefreshIntervalMinutes: 60,
  collectorTimeoutSeconds: 15,
  collectorMaxConcurrency: 3,
  allowDepletedFallback: false,
  developerModeEnabled: false,
} satisfies UpdateSettingsInputDto;

describe("generated settings/stations transport envelopes", () => {
  beforeEach(() => {
    transport.invoke.mockReset().mockResolvedValue(undefined);
    transport.invokeNonIdempotent.mockReset().mockResolvedValue(undefined);
  });

  it("sends every migrated command with the Tauri { input } envelope", async () => {
    await getSettings();
    await listStations();
    await updateSettings(settingsInput);
    await createStation(stationInput);
    await updateStation({ ...stationInput, id: "station-1", apiKey: null });
    await deleteStation({ id: "station-1" });
    await reorderStations({ stationIds: ["station-1"] });

    expect(transport.invoke.mock.calls).toEqual([
      ["get_settings", { input: {} }],
      ["list_stations", { input: {} }],
      ["update_settings", { input: settingsInput }],
      ["update_station", { input: { ...stationInput, id: "station-1", apiKey: null } }],
      ["delete_station", { input: { id: "station-1" } }],
      ["reorder_stations", { input: { stationIds: ["station-1"] } }],
    ]);
    expect(transport.invokeNonIdempotent).toHaveBeenCalledExactlyOnceWith(
      "create_station",
      { input: stationInput },
    );
  });

  it("sends every ordinary station-key command through its generated transport policy", async () => {
    const stationId = "station-1";
    const keyId = "key-1";
    const remoteKeyId = "remote-1";
    const createInput = {
      stationId,
      name: "Fixture key",
      apiKey: "fixture-not-a-real-api-key",
      enabled: true,
      groupName: null,
      tierLabel: null,
      note: null,
    };
    const updateInput = {
      ...createInput,
      id: keyId,
      apiKey: null,
      priority: 0,
      maxConcurrency: 3,
      schedulable: true,
      status: "unchecked" as const,
    };
    const createRemoteInput = {
      stationId,
      name: "Fixture remote key",
      groupBindingId: null,
      groupIdHash: null,
      groupName: null,
    };
    const credentialsInput = {
      stationId,
      loginUsername: "fixture-user",
      loginPassword: "fixture-not-a-real-password",
      rememberPassword: false,
    };
    const sessionInput = {
      stationId,
      accessToken: "fixture-not-a-real-access-token",
      refreshToken: null,
      cookie: null,
      newapiUserId: null,
      tokenExpiresAt: null,
    };
    const saveInput = {
      mode: "create" as const,
      id: null,
      stationId,
      name: "Fixture defaults",
      apiKey: "fixture-not-a-real-api-key",
      enabled: true,
      groupSelection: { kind: "clear" as const },
    };

    await listStationKeys({ stationId });
    await updateStationKey(updateInput);
    await updateStationKeyGroupBinding({ stationKeyId: keyId, groupBindingId: "group-1" });
    await deleteStationKey({ id: keyId });
    await reorderStationKeys({ stationId, keyIds: [keyId] });
    await getRemoteKeyCapability({ stationId });
    await listRemoteStationKeys({ stationId });
    await scanRemoteStationKeys({ stationId });
    await bindRemoteStationKey({ remoteKeyId, stationKeyId: keyId });
    await unbindRemoteStationKey({ remoteKeyId, stationId });
    await listKeyPoolItems();
    await reorderKeyPool({ keyIds: [keyId] });
    await getStationCredentials({ stationId });
    await updateStationCredentials(credentialsInput);
    await updateStationSession(sessionInput);
    await clearStationCredentials({ stationId });
    await createStationKey(createInput);
    await saveStationKeyWithDefaults(saveInput);
    await createRemoteStationKey(createRemoteInput);
    await createLocalStationKeyFromRemote({ remoteKeyId, stationId });

    expect(transport.invoke.mock.calls).toEqual([
      ["list_station_keys", { input: { stationId } }],
      ["update_station_key", { input: updateInput }],
      ["update_station_key_group_binding", { input: { stationKeyId: keyId, groupBindingId: "group-1" } }],
      ["delete_station_key", { input: { id: keyId } }],
      ["reorder_station_keys", { input: { stationId, keyIds: [keyId] } }],
      ["get_remote_key_capability", { input: { stationId } }],
      ["list_remote_station_keys", { input: { stationId } }],
      ["scan_remote_station_keys", { input: { stationId } }],
      ["bind_remote_station_key", { input: { remoteKeyId, stationKeyId: keyId } }],
      ["unbind_remote_station_key", { input: { remoteKeyId, stationId } }],
      ["list_key_pool_items", { input: {} }],
      ["reorder_key_pool", { input: { keyIds: [keyId] } }],
      ["get_station_credentials", { input: { stationId } }],
      ["update_station_credentials", { input: credentialsInput }],
      ["update_station_session", { input: sessionInput }],
      ["clear_station_credentials", { input: { stationId } }],
    ]);
    expect(transport.invokeNonIdempotent.mock.calls).toEqual([
      ["create_station_key", { input: createInput }],
      ["save_station_key_with_defaults", { input: saveInput }],
      ["create_remote_station_key", { input: createRemoteInput }],
      ["create_local_station_key_from_remote", { input: { remoteKeyId, stationId } }],
    ]);
  });

  it("sends every changes/logs command through generated envelopes", async () => {
    const input = {
      severity: "warning" as const,
      eventType: "fixture.changed",
      title: "Fixture change",
      message: "Fixture message",
      objectType: "station",
      objectId: "station-1",
      stationId: "station-1",
      stationKeyId: null,
      pricingRuleId: null,
      requestLogId: null,
      oldValueJson: null,
      newValueJson: "{}",
      impactJson: null,
      dedupeKey: "fixture-change-1",
      source: "fixture",
    };

    await listRequestLogs();
    await clearRequestLogs();
    await listChangeEvents();
    await clearChangeEvents();
    await listChangeEventsForStation({ stationId: "station-1" });
    await upsertChangeEvent(input);
    await markChangeEventRead({ id: "change-1" });
    await markChangeEventsRead({ ids: ["change-1", "change-2"] });
    await dismissChangeEvent({ id: "change-1" });
    await resolveChangeEvent({ id: "change-1" });

    expect(transport.invoke.mock.calls.slice(-10)).toEqual([
      ["list_request_logs", { input: {} }],
      ["clear_request_logs", { input: {} }],
      ["list_change_events", { input: {} }],
      ["clear_change_events", { input: {} }],
      ["list_change_events_for_station", { input: { stationId: "station-1" } }],
      ["upsert_change_event", { input }],
      ["mark_change_event_read", { input: { id: "change-1" } }],
      ["mark_change_events_read", { input: { ids: ["change-1", "change-2"] } }],
      ["dismiss_change_event", { input: { id: "change-1" } }],
      ["resolve_change_event", { input: { id: "change-1" } }],
    ]);
  });

  it("sends every collector facts/snapshots command through generated envelopes", async () => {
    const stationId = "station-1";
    const balanceInput = {
      id: null,
      stationId,
      stationKeyId: null,
      scope: "station" as const,
      value: 12.5,
      currency: "CNY",
      creditUnit: null,
      usedValue: null,
      totalValue: null,
      todayRequestCount: null,
      totalRequestCount: null,
      todayConsumption: null,
      totalConsumption: null,
      todayBaseConsumption: null,
      totalBaseConsumption: null,
      todayTokenCount: null,
      totalTokenCount: null,
      todayInputTokenCount: null,
      todayOutputTokenCount: null,
      totalInputTokenCount: null,
      totalOutputTokenCount: null,
      accountConcurrencyLimit: null,
      lowBalanceThreshold: 5,
      status: "normal" as const,
      source: "fixture",
      confidence: 0.9,
      collectedAt: "1700000000000",
    };
    const bindingInput = {
      stationId,
      stationKeyId: null,
      bindingKind: "station_group" as const,
      parentGroupBindingId: null,
      groupKeyHash: "group-hash-1",
      groupIdHash: "group-id-hash-1",
      groupName: "default",
      bindingStatus: "available" as const,
      defaultRateMultiplier: 1,
      userRateMultiplier: null,
      effectiveRateMultiplier: 1,
      inferredGroupCategory: "gpt" as const,
      groupCategoryOverride: null,
      rateSource: "fixture",
      confidence: 0.9,
      lastSeenAt: "1700000000000",
      rawJsonRedacted: null,
    };

    await listBalanceSnapshots();
    await listCurrentStationBalanceSnapshots();
    await listBalanceSnapshotsForStation({ stationId });
    await upsertBalanceSnapshot(balanceInput);
    await listStationGroupBindings({ stationId });
    await listStationGroupOptions({ stationId });
    await upsertStationGroupBinding(bindingInput);
    await listGroupRateRecords({ stationId });
    await listCollectorRuns({ stationId });
    await listCollectorSnapshots({ stationId });
    await getLatestCollectorSnapshot({ stationId });

    expect(transport.invoke.mock.calls.slice(-11)).toEqual([
      ["list_balance_snapshots", { input: {} }],
      ["list_current_station_balance_snapshots", { input: {} }],
      ["list_balance_snapshots_for_station", { input: { stationId } }],
      ["upsert_balance_snapshot", { input: balanceInput }],
      ["list_station_group_bindings", { input: { stationId } }],
      ["list_station_group_options", { input: { stationId } }],
      ["upsert_station_group_binding", { input: bindingInput }],
      ["list_group_rate_records", { input: { stationId } }],
      ["list_collector_runs", { input: { stationId } }],
      ["list_collector_snapshots", { input: { stationId } }],
      ["get_latest_collector_snapshot", { input: { stationId } }],
    ]);
  });

  it("sends every channel-monitor read through generated envelopes", async () => {
    await listChannelMonitors();
    await listChannelMonitorSummaries({ runSince: "1700000000000", runLimit: 60 });
    await listChannelStatusSummaries();
    await listChannelMonitorRuns({ monitorId: "monitor-1" });
    await listChannelMonitorTemplates();

    expect(transport.invoke.mock.calls.slice(-5)).toEqual([
      ["list_channel_monitors", { input: {} }],
      ["list_channel_monitor_summaries", { input: { runSince: "1700000000000", runLimit: 60 } }],
      ["list_channel_status_summaries", { input: {} }],
      ["list_channel_monitor_runs", { input: { monitorId: "monitor-1" } }],
      ["list_channel_monitor_templates", { input: {} }],
    ]);
  });

  it("sends every channel-monitor mutation through generated envelopes", async () => {
    const monitorInput = {
      name: "Fixture monitor",
      targetType: "station_key" as const,
      stationId: "station-1",
      stationKeyId: "key-1",
      templateId: "template-1",
      enabled: true,
      intervalSeconds: 60,
      jitterSeconds: 5,
      timeoutSeconds: 15,
      maxConcurrency: 1,
      consecutiveFailureThreshold: 3,
      fallbackModels: ["fixture-model"],
      note: null,
    };
    const templateInput = {
      name: "Fixture template",
      endpointKind: "chat_completions",
      method: "POST",
      path: "/v1/chat/completions",
      requestBodyJson: "{}",
      enabled: true,
      note: null,
    };

    await createChannelMonitor(monitorInput);
    await updateChannelMonitor({ id: "monitor-1", ...monitorInput });
    await deleteChannelMonitor({ id: "monitor-1" });
    await createChannelMonitorTemplate(templateInput);
    await updateChannelMonitorTemplate({ id: "template-1", ...templateInput });
    await duplicateChannelMonitorTemplate({ id: "template-1" });
    await deleteChannelMonitorTemplate({ id: "template-1" });

    expect(transport.invoke.mock.calls.slice(-4)).toEqual([
      ["update_channel_monitor", { input: { id: "monitor-1", ...monitorInput } }],
      ["delete_channel_monitor", { input: { id: "monitor-1" } }],
      ["update_channel_monitor_template", { input: { id: "template-1", ...templateInput } }],
      ["delete_channel_monitor_template", { input: { id: "template-1" } }],
    ]);
    expect(transport.invokeNonIdempotent.mock.calls).toEqual([
      ["create_channel_monitor", { input: monitorInput }],
      ["create_channel_monitor_template", { input: templateInput }],
      ["duplicate_channel_monitor_template", { input: { id: "template-1" } }],
    ]);
  });

  it("sends channel-monitor operations through generated envelopes", async () => {
    await loadChannelStatusWorkspace();
    await runChannelMonitorNow({ monitorId: "monitor-1" });

    expect(transport.invoke.mock.calls.slice(-1)).toEqual([
      ["load_channel_status_workspace", { input: {} }],
    ]);
    expect(transport.invokeNonIdempotent.mock.calls).toEqual([
      ["run_channel_monitor_now", { input: { monitorId: "monitor-1" } }],
    ]);
  });

  it("sends station collector operations through their frozen transport policies", async () => {
    const stationId = "station-1";
    await detectSub2apiStation({ stationId });
    await collectSub2apiStation({ stationId });
    await detectStationInfo({ stationId });
    await collectStationInfo({ stationId });
    await collectStationTask({ stationId, taskType: "groups" });
    await testStationLogin({ stationId });
    await testStationLoginInput({
      stationType: "newapi",
      websiteUrl: "https://example.test",
      loginUsername: "fixture-user",
      loginPassword: "not-a-real-secret",
    });

    expect(transport.invokeNonIdempotent.mock.calls).toEqual([
      ["detect_sub2api_station", { input: { stationId } }],
      ["collect_sub2api_station", { input: { stationId } }],
      ["detect_station_info", { input: { stationId } }],
      ["collect_station_info", { input: { stationId } }],
      ["collect_station_task", { input: { stationId, taskType: "groups" } }],
      ["test_station_login", { input: { stationId } }],
    ]);
    expect(transport.invoke.mock.calls.slice(-1)).toEqual([
      [
        "test_station_login_input",
        {
          input: {
            stationType: "newapi",
            websiteUrl: "https://example.test",
            loginUsername: "fixture-user",
            loginPassword: "not-a-real-secret",
          },
        },
      ],
    ]);
  });

  it("sends routing and health reads through generated envelopes", async () => {
    await getStationKeyCapabilities({ stationKeyId: "key-1" });
    await listModelAliases();
    await listStationKeyHealth();
    await listStationEndpointHealth();
    await getStationKeyHealth({ stationKeyId: "key-1" });
    await simulateRoute({
      endpoint: "chat_completions",
      model: "fixture-model",
      stream: true,
      usesTools: false,
      usesVision: false,
      usesReasoning: false,
      policy: "cost_stable_first",
      maxRateMultiplier: 2,
      routingGroupFilter: { group_type: "gpt" },
      sessionHash: "session-1",
      previousResponseId: null,
    });

    expect(transport.invoke.mock.calls).toEqual([
      ["get_station_key_capabilities", { input: { stationKeyId: "key-1" } }],
      ["list_model_aliases", { input: {} }],
      ["list_station_key_health", { input: {} }],
      ["list_station_endpoint_health", { input: {} }],
      ["get_station_key_health", { input: { stationKeyId: "key-1" } }],
      [
        "simulate_route",
        {
          input: {
            endpoint: "chat_completions",
            model: "fixture-model",
            stream: true,
            usesTools: false,
            usesVision: false,
            usesReasoning: false,
            policy: "cost_stable_first",
            maxRateMultiplier: 2,
            routingGroupFilter: { group_type: "gpt" },
            sessionHash: "session-1",
            previousResponseId: null,
          },
        },
      ],
    ]);
  });

  it("sends routing mutations through generated envelopes", async () => {
    const capabilities = {
      stationKeyId: "key-1",
      supportsChatCompletions: true,
      supportsResponses: true,
      supportsEmbeddings: false,
      supportsStream: true,
      supportsTools: false,
      supportsVision: false,
      supportsReasoning: false,
      modelAllowlist: ["fixture-model"],
      modelBlocklist: [],
      preferredModels: ["fixture-model"],
      onlyUseAsBackup: false,
      routingTags: ["fixture"],
    };
    const alias = {
      id: null,
      clientModel: "client-model",
      upstreamModel: "upstream-model",
      enabled: true,
      note: null,
    };

    await updateStationKeyCapabilities(capabilities);
    await upsertModelAlias(alias);
    await deleteModelAlias({ id: "alias-1" });

    expect(transport.invoke.mock.calls).toEqual([
      ["update_station_key_capabilities", { input: capabilities }],
      ["upsert_model_alias", { input: alias }],
      ["delete_model_alias", { input: { id: "alias-1" } }],
    ]);
  });

  it("sends pricing reads through generated envelopes", async () => {
    await listPricingRules();
    await listModelBasePrices();
    await resolveStationKeyPricingContext({
      stationKeyId: "key-1",
      requestedModel: "fixture-model",
      requestKind: "text",
    });
    await loadPricingComparisonWorkspace();

    expect(transport.invoke.mock.calls).toEqual([
      ["list_pricing_rules", { input: {} }],
      ["list_model_base_prices", { input: {} }],
      [
        "resolve_station_key_pricing_context",
        {
          input: {
            stationKeyId: "key-1",
            requestedModel: "fixture-model",
            requestKind: "text",
          },
        },
      ],
      ["load_pricing_comparison_workspace", { input: {} }],
    ]);
  });

  it("sends pricing mutations through generated envelopes", async () => {
    const basePrice = {
      id: null,
      provider: "openai",
      model: "fixture-model",
      inputPrice: 1,
      outputPrice: 2,
      currency: "USD",
      unit: "M",
      sourceUrl: "https://example.test/pricing",
      sourceLabel: "Fixture",
      sourceCheckedAt: null,
      enabled: true,
      builtIn: false,
      note: null,
    };
    const rule = {
      id: null,
      stationId: "station-1",
      stationKeyId: null,
      groupBindingId: null,
      groupName: null,
      tierLabel: null,
      model: "fixture-model",
      inputPrice: 1,
      outputPrice: 2,
      fixedPrice: null,
      rateMultiplier: 1,
      currency: "USD",
      unit: "M",
      priceType: "token",
      basePriceSource: null,
      normalizationStatus: null,
      source: "manual",
      confidence: 1,
      enabled: true,
      note: null,
      collectedAt: null,
      validFrom: null,
      validUntil: null,
    };

    await upsertModelBasePrice(basePrice);
    await resetModelBasePricesToBuiltins();
    await upsertPricingRule(rule);
    await deletePricingRule({ id: "rule-1" });

    expect(transport.invoke.mock.calls).toEqual([
      ["upsert_model_base_price", { input: basePrice }],
      ["reset_model_base_prices_to_builtins", { input: {} }],
      ["upsert_pricing_rule", { input: rule }],
      ["delete_pricing_rule", { input: { id: "rule-1" } }],
    ]);
  });

  it("sends proxy workspace reads through generated envelopes", async () => {
    await getProxyStatus();
    await loadLocalRoutingWorkspace();

    expect(transport.invoke.mock.calls).toEqual([
      ["get_proxy_status", { input: {} }],
      ["load_local_routing_workspace", { input: {} }],
    ]);
  });
});
