import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const sub2ApiCommit = "1e618dbc299fc0a82e9a690bcf2d5843be817113";
const expectedBuiltinCount = 198;
const sourceLabel = "Sub2API model pricing catalog";

const rustCatalogFile = await readFile("src-tauri/src/services/pricing_catalog.rs", "utf8");
const rustStoreFile = await readFile("src-tauri/src/persistence/stores/pricing_store.rs", "utf8");
const rustCatalogSource = extractBetween(
  rustCatalogFile,
  "pub(crate) const BUILTIN_MODEL_BASE_PRICE_CHECKED_AT",
  "pub(crate) struct StaticBuiltinModelBasePriceCatalog",
);
const rustSeedSource = extractBetween(
  rustStoreFile,
  "pub(crate) async fn reset_model_base_prices_to_builtins",
  "pub(crate) async fn ensure_builtin_model_base_prices",
);

const rustRows = parseRustRows(rustCatalogSource);

assert.equal(
  rustRows.length,
  expectedBuiltinCount,
  "Rust builtin model base prices should mirror the Sub2API pricing catalog row count",
);

assert.ok(
  rustSeedSource.includes("DELETE FROM model_base_prices WHERE built_in = 1"),
  "resetting builtins should remove stale old builtin rows before inserting the Sub2API catalog",
);

assert.ok(rustCatalogSource.includes(sub2ApiCommit), "builtin catalog should pin the exact Sub2API source commit");
assert.ok(rustCatalogSource.includes(sourceLabel), "builtin catalog should identify Sub2API as the pricing source");
assert.ok(rustCatalogSource.includes("2026-08-12"), "builtin catalog should record the Sub2API source check date");
assert.ok(!rustCatalogSource.includes("per_1m_tokens"), "builtin catalog should not use the old per_1m_tokens unit");
assert.equal(
  [...rustCatalogSource.matchAll(new RegExp(sub2ApiCommit, "g"))].length,
  expectedBuiltinCount,
  "every builtin row should pin the same Sub2API source commit",
);

assert.ok(rustCatalogFile.includes('unit: "M".to_string()'), "Rust catalog adapter should use the short M unit label");
assert.ok(!rustCatalogFile.includes("per_1m_tokens"), "Rust catalog should not use the old per_1m_tokens unit");

for (const row of [
  ["openai", "gpt-5.5", 5, 30],
  ["openai", "gpt-5.4", 2.5, 15],
  ["openai", "gpt-5.4-mini", 0.75, 4.5],
  ["openai", "gpt-5.4-nano", 0.2, 1.25],
  ["anthropic", "claude-opus-4-5", 5, 25],
  ["anthropic", "claude-opus-5", 5, 25],
  ["anthropic", "claude-sonnet-4-5", 3, 15],
  ["anthropic", "claude-haiku-4-5", 1, 5],
  ["google", "gemini-2.5-pro", 1.25, 10],
  ["google", "gemini-3.6-flash", 1.5, 7.5],
  ["openai", "gpt-image-1", 5, null],
  ["openai", "codex-auto-review", 0.2, 1.2],
  ["openai", "gpt-5.6-luna", 0.2, 1.2],
  ["openai", "gpt-5.6-terra", 2, 12],
  ["deepseek", "deepseek-chat", 0.28, 0.42],
]) {
  assertCatalogRow(rustRows, "Rust builtin seed", row);
}

assertCatalogFields(rustRows, "openai", "gpt-5.4", {
  cacheReadPrice: 0.25,
  cacheReadPricePriority: 0.5,
  supportsServiceTier: true,
  supportsPromptCaching: true,
});
assertCatalogFields(rustRows, "openai", "gpt-5.6-terra", {
  inputPricePriority: 4,
  outputPricePriority: 24,
  cacheCreationPrice: 2.5,
  cacheCreationPricePriority: 5,
  cacheReadPrice: 0.2,
  cacheReadPricePriority: 0.4,
  longContextInputTokenThreshold: 272000,
  longContextInputCostMultiplier: 2,
  longContextOutputCostMultiplier: 1.5,
});
assertCatalogFields(rustRows, "anthropic", "claude-opus-4-5", {
  cacheCreationPrice: 6.25,
  cacheCreationPriceAbove1Hr: 10,
  cacheReadPrice: 0.5,
  supportsPromptCaching: true,
});

function assertCatalogRow(rows, label, [provider, model, inputPrice, outputPrice]) {
  const row = rows.find((candidate) => candidate.provider === provider && candidate.model === model);
  assert.ok(row, `${label} should include ${provider}/${model}`);
  assert.equal(row.inputPrice, inputPrice, `${label} input price for ${model}`);
  assert.equal(row.outputPrice, outputPrice, `${label} output price for ${model}`);
}

function assertCatalogFields(rows, provider, model, expected) {
  const row = rows.find((candidate) => candidate.provider === provider && candidate.model === model);
  assert.ok(row, `Rust builtin seed should include ${provider}/${model}`);
  for (const [field, value] of Object.entries(expected)) {
    assert.equal(row[field], value, `Rust builtin seed ${field} for ${model}`);
  }
}

function parseRustRows(source) {
  return [...source.matchAll(/BuiltinModelBasePrice\s*\{([\s\S]*?)\n\s*\},/g)].map((match) => {
    const block = match[1];
    return {
      id: rustString(block, "id"),
      provider: rustString(block, "provider"),
      model: rustString(block, "model"),
      inputPrice: rustOption(block, "input_price"),
      outputPrice: rustOption(block, "output_price"),
      inputPricePriority: rustOption(block, "input_price_priority"),
      outputPricePriority: rustOption(block, "output_price_priority"),
      cacheCreationPrice: rustOption(block, "cache_creation_price"),
      cacheCreationPricePriority: rustOption(block, "cache_creation_price_priority"),
      cacheCreationPriceAbove1Hr: rustOption(block, "cache_creation_price_above_1hr"),
      cacheReadPrice: rustOption(block, "cache_read_price"),
      cacheReadPricePriority: rustOption(block, "cache_read_price_priority"),
      longContextInputTokenThreshold: rustOption(block, "long_context_input_token_threshold"),
      longContextInputCostMultiplier: rustOption(block, "long_context_input_cost_multiplier"),
      longContextOutputCostMultiplier: rustOption(block, "long_context_output_cost_multiplier"),
      supportsServiceTier: rustBoolean(block, "supports_service_tier"),
      supportsPromptCaching: rustBoolean(block, "supports_prompt_caching"),
    };
  });
}

function rustString(block, field) {
  const match = block.match(new RegExp(`\\b${field}:\\s*"([^"]+)"`));
  assert.ok(match, `missing Rust string field ${field}`);
  return match[1];
}

function rustOption(block, field) {
  const match = block.match(new RegExp(`\\b${field}:\\s*(?:Some\\(([-+0-9.eE]+)\\)|None)`));
  assert.ok(match, `missing Rust option field ${field}`);
  return match[1] == null ? null : Number(match[1]);
}

function rustBoolean(block, field) {
  const match = block.match(new RegExp(`\\b${field}:\\s*(true|false)`));
  assert.ok(match, `missing Rust boolean field ${field}`);
  return match[1] === "true";
}

function extractBetween(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start);
  assert.notEqual(start, -1, `missing start marker ${startMarker}`);
  assert.notEqual(end, -1, `missing end marker ${endMarker}`);
  return source.slice(start, end);
}
