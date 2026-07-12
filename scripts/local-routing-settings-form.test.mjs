import assert from "node:assert/strict";
import { createServer } from "vite";

const vite = await createServer({
  appType: "custom",
  logLevel: "silent",
  server: { middlewareMode: true },
});

try {
  const formModule = await vite.ssrLoadModule(
    "/src/features/routing/localRoutingSettingsForm.ts",
  );
  const settingsModule = await vite.ssrLoadModule("/src/lib/types/settings.ts");
  const {
    createLocalRoutingSettingsDraft,
    parseLocalRoutingBoundaryDraft,
    parseLocalRoutingSettingsDraft,
  } = formModule;
  const { DEFAULT_SCHEDULER_ADVANCED_SETTINGS, appSettingsToUpdateInput } = settingsModule;

  const baseSettings = {
    localProxyPort: 8787,
    localKeyMasked: "sk-local-****",
    defaultRoutingStrategy: "automatic_balanced",
    collectorProxyMode: "direct",
    collectorProxyUrl: null,
    maxRateMultiplier: 2,
    defaultRoutingGroupFilter: "all_groups",
    schedulerAdvancedSettings: { ...DEFAULT_SCHEDULER_ADVANCED_SETTINGS },
    lowBalanceThresholdCny: 15,
    collectorIntervalMinutes: 30,
    balanceIntervalMinutes: 5,
    groupRateIntervalMinutes: 20,
    modelListIntervalMinutes: 60,
    pricingRefreshIntervalMinutes: 60,
    collectorTimeoutSeconds: 15,
    collectorMaxConcurrency: 3,
    allowDepletedFallback: false,
    trayBehavior: "minimize-to-tray",
    developerModeEnabled: false,
    dataDir: "C:/relay-pool",
    pendingDataDir: null,
    dataDirChangeRequiresRestart: false,
  };

  const validDraft = createLocalRoutingSettingsDraft(baseSettings);
  assert.equal(validDraft.maxRateLimitEnabled, true);
  assert.equal(validDraft.lowBalanceThresholdCny, "15");
  assert.equal(validDraft.allowDepletedFallback, false);

  const validResult = parseLocalRoutingSettingsDraft(validDraft);
  assert.equal(validResult.ok, true);
  assert.equal(validResult.value.maxRateMultiplier, 2);
  assert.equal(validResult.value.lowBalanceThresholdCny, 15);
  assert.equal(validResult.value.allowDepletedFallback, false);
  assert.deepEqual(
    validResult.value.schedulerAdvancedSettings,
    DEFAULT_SCHEDULER_ADVANCED_SETTINGS,
  );

  const noCeilingResult = parseLocalRoutingSettingsDraft({
    ...validDraft,
    maxRateLimitEnabled: false,
    maxRateMultiplier: "",
  });
  assert.equal(noCeilingResult.ok, true);
  assert.equal(noCeilingResult.value.maxRateMultiplier, null);

  for (const maxRateMultiplier of ["", "not-a-number", "-1"]) {
    const result = parseLocalRoutingBoundaryDraft({
      ...validDraft,
      maxRateLimitEnabled: true,
      maxRateMultiplier,
    });
    assert.equal(result.ok, false);
    assert.match(result.errors.maxRateMultiplier, /倍率上限/);
  }

  for (const lowBalanceThresholdCny of ["", "-0.01", "not-a-number"]) {
    const result = parseLocalRoutingSettingsDraft({
      ...validDraft,
      lowBalanceThresholdCny,
    });
    assert.equal(result.ok, false);
    assert.match(result.errors.lowBalanceThresholdCny, /低余额阈值/);
  }

  const depletedFallbackDraft = createLocalRoutingSettingsDraft({
    ...baseSettings,
    allowDepletedFallback: true,
  });
  const depletedFallbackResult = parseLocalRoutingSettingsDraft(depletedFallbackDraft);
  assert.equal(depletedFallbackResult.ok, true);
  assert.equal(depletedFallbackResult.value.allowDepletedFallback, true);

  const specificFilter = { group_binding_id: "binding-1" };
  const specificDraft = createLocalRoutingSettingsDraft({
    ...baseSettings,
    defaultRoutingGroupFilter: specificFilter,
  });
  const specificResult = parseLocalRoutingSettingsDraft(specificDraft);
  assert.equal(specificResult.ok, true);
  assert.deepEqual(specificResult.value.defaultRoutingGroupFilter, specificFilter);

  const boundaryResult = parseLocalRoutingBoundaryDraft(validDraft);
  assert.equal(boundaryResult.ok, true);
  assert.equal(boundaryResult.value.maxRateMultiplier, 2);
  assert.deepEqual(boundaryResult.value.defaultRoutingGroupFilter, "all_groups");
  assert.equal(boundaryResult.value.lowBalanceThresholdCny, 15);
  assert.equal(boundaryResult.value.allowDepletedFallback, false);
  assert.equal(
    boundaryResult.value.schedulerAdvancedPatch.multiplierMinConfidence,
    DEFAULT_SCHEDULER_ADVANCED_SETTINGS.multiplierMinConfidence,
  );

  for (const topK of ["0", "1.5", "65536"]) {
    const result = parseLocalRoutingSettingsDraft({
      ...validDraft,
      scheduler: { ...validDraft.scheduler, topK },
    });
    assert.equal(result.ok, false);
    assert.match(result.errors.topK, /1.*65535/);
  }

  const negativeWeight = parseLocalRoutingSettingsDraft({
    ...validDraft,
    scheduler: { ...validDraft.scheduler, load: "-0.1" },
  });
  assert.equal(negativeWeight.ok, false);
  assert.match(negativeWeight.errors.load, /大于或等于 0/);

  for (const [field, value] of [
    ["multiplierMinConfidence", "1.1"],
    ["stickyEscapeErrorRate", "-0.1"],
  ]) {
    const result = parseLocalRoutingSettingsDraft({
      ...validDraft,
      scheduler: { ...validDraft.scheduler, [field]: value },
    });
    assert.equal(result.ok, false);
    assert.match(result.errors[field], /0 到 1/);
  }

  const zeroBaseScheduler = { ...validDraft.scheduler };
  for (const field of [
    "multiplier",
    "priority",
    "load",
    "queue",
    "errorRate",
    "ttft",
    "quotaHeadroom",
  ]) {
    zeroBaseScheduler[field] = "0";
  }
  const zeroBaseResult = parseLocalRoutingSettingsDraft({
    ...validDraft,
    scheduler: zeroBaseScheduler,
  });
  assert.equal(zeroBaseResult.ok, false);
  assert.match(zeroBaseResult.errors.baseWeights, /至少保留一个/);

  const zeroWaiting = parseLocalRoutingSettingsDraft({
    ...validDraft,
    scheduler: { ...validDraft.scheduler, fallbackMaxWaiting: "0" },
  });
  assert.equal(zeroWaiting.ok, false);
  assert.match(zeroWaiting.errors.fallbackMaxWaiting, /大于 0/);

  const updateInput = appSettingsToUpdateInput(baseSettings);
  assert.equal(updateInput.localProxyPort, baseSettings.localProxyPort);
  assert.equal(updateInput.collectorMaxConcurrency, baseSettings.collectorMaxConcurrency);
  assert.equal("dataDir" in updateInput, false);
  assert.equal("localKeyMasked" in updateInput, false);

  console.log("local routing settings form behavior ok");
} finally {
  await vite.close();
}
