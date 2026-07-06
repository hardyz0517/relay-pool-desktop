import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import ts from "typescript";

async function importTsModule(path) {
  const source = await readFile(path, "utf8");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  const encoded = Buffer.from(output, "utf8").toString("base64");
  return import(`data:text/javascript;base64,${encoded}`);
}

const { rateRowsFromGroupFacts } = await importTsModule("src/features/pricing/rateSnapshotParser.ts");
const { buildGroupRateOnlyPricingRules } = await importTsModule("src/features/pricing/pricingMatrix.ts");

const rows = rateRowsFromGroupFacts(
  [
    {
      id: "group-openai",
      stationId: "station-1",
      stationKeyId: null,
      bindingKind: "station_group",
      parentGroupBindingId: null,
      groupKeyHash: "hash-openai",
      groupIdHash: "default",
      groupName: "default",
      bindingStatus: "available",
      defaultRateMultiplier: 1,
      userRateMultiplier: 0.8,
      effectiveRateMultiplier: 0.8,
      rateSource: "sub2api_groups_rates",
      confidence: 0.9,
      lastSeenAt: null,
      lastCheckedAt: "2026-07-06T00:00:00.000Z",
      lastRateChangedAt: null,
      rawJsonRedacted: { color: "green", tag: "OpenAI" },
      createdAt: "2026-07-06T00:00:00.000Z",
      updatedAt: "2026-07-06T00:00:00.000Z",
    },
    {
      id: "group-claude",
      stationId: "station-1",
      stationKeyId: null,
      bindingKind: "station_group",
      parentGroupBindingId: null,
      groupKeyHash: "hash-claude",
      groupIdHash: "claude",
      groupName: "claude",
      bindingStatus: "available",
      defaultRateMultiplier: 1.2,
      userRateMultiplier: 1.1,
      effectiveRateMultiplier: 1.1,
      rateSource: "sub2api_groups_rates",
      confidence: 0.9,
      lastSeenAt: null,
      lastCheckedAt: "2026-07-06T00:00:00.000Z",
      lastRateChangedAt: null,
      rawJsonRedacted: { color: "yellow", badge: "Anthropic" },
      createdAt: "2026-07-06T00:00:00.000Z",
      updatedAt: "2026-07-06T00:00:00.000Z",
    },
  ],
  [],
);

const openaiRow = rows.find((row) => row.groupName === "default");
assert.equal(openaiRow?.modelProvider, "openai");
assert.deepEqual(openaiRow?.modelPrefixes, ["gpt-", "o1", "o3", "o4", "chatgpt-"]);

const claudeRow = rows.find((row) => row.groupName === "claude");
assert.equal(claudeRow?.modelProvider, "anthropic");
assert.deepEqual(claudeRow?.modelPrefixes, ["claude-"]);

const syntheticRules = buildGroupRateOnlyPricingRules(rows, [{ id: "station-1", name: "Relay A" }], []);
assert.ok(
  syntheticRules.some(
    (rule) =>
      rule.model === "gpt-*" &&
      rule.groupName === "default" &&
      rule.normalizationStatus === "group_rate_only" &&
      rule.rateMultiplier === 0.8,
  ),
  "OpenAI green/default group should produce a gpt-* rate-only model row",
);
assert.ok(
  syntheticRules.some(
    (rule) =>
      rule.model === "claude-*" &&
      rule.groupName === "claude" &&
      rule.normalizationStatus === "group_rate_only" &&
      rule.rateMultiplier === 1.1,
  ),
  "Anthropic yellow group should produce a claude-* rate-only model row",
);

