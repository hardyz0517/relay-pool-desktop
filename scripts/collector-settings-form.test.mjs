import assert from "node:assert/strict";
import { createServer } from "vite";

const vite = await createServer({
  appType: "custom",
  logLevel: "silent",
  server: { middlewareMode: true },
});

try {
  const formModule = await vite.ssrLoadModule(
    "/src/features/collectors/collectorSettingsForm.ts",
  );
  const {
    applyCollectorFrequencyPreset,
    createCollectorSettingsDraft,
    detectCollectorFrequencyPreset,
    parseCollectorSettingsDraft,
  } = formModule;

  const settings = {
    balanceIntervalMinutes: 5,
    groupRateIntervalMinutes: 20,
    modelListIntervalMinutes: 60,
    pricingRefreshIntervalMinutes: 60,
    collectorTimeoutSeconds: 15,
    collectorMaxConcurrency: 3,
  };

  const draft = createCollectorSettingsDraft(settings);
  assert.equal(detectCollectorFrequencyPreset(draft), "balanced");

  const timely = applyCollectorFrequencyPreset(draft, "timely");
  assert.deepEqual(
    {
      balance: timely.balanceIntervalMinutes,
      groupRate: timely.groupRateIntervalMinutes,
      models: timely.modelListIntervalMinutes,
      pricing: timely.pricingRefreshIntervalMinutes,
    },
    { balance: "2", groupRate: "10", models: "30", pricing: "30" },
  );
  assert.equal(timely.collectorTimeoutSeconds, "15");
  assert.equal(timely.collectorMaxConcurrency, "3");

  const resourceSaver = applyCollectorFrequencyPreset(draft, "resource_saver");
  assert.equal(resourceSaver.balanceIntervalMinutes, "15");
  assert.equal(resourceSaver.groupRateIntervalMinutes, "60");
  assert.equal(resourceSaver.modelListIntervalMinutes, "180");
  assert.equal(resourceSaver.pricingRefreshIntervalMinutes, "180");

  const valid = parseCollectorSettingsDraft(draft);
  assert.equal(valid.ok, true);
  assert.equal(valid.value.collectorMaxConcurrency, 3);

  for (const [field, value] of [
    ["balanceIntervalMinutes", "0"],
    ["groupRateIntervalMinutes", "1.5"],
    ["collectorTimeoutSeconds", "2"],
    ["collectorMaxConcurrency", "9"],
  ]) {
    const result = parseCollectorSettingsDraft({ ...draft, [field]: value });
    assert.equal(result.ok, false);
    assert.ok(result.errors[field]);
  }

  console.log("collector settings form behavior ok");
} finally {
  await vite.close();
}
