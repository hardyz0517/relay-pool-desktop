import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const ccSwitchCommit = "9a596158ca926e74b56243c08af67d9dd13fc27c";
const expectedBuiltinCount = 192;
const sourceLabel = "CC Switch built-in model pricing";

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
  "Rust builtin model base prices should mirror the pinned CC Switch seed row count",
);

assert.ok(
  rustSeedSource.includes("DELETE FROM model_base_prices WHERE built_in = 1"),
  "resetting builtins should remove stale old builtin rows before inserting the CC Switch catalog",
);

assert.ok(rustCatalogFile.includes(ccSwitchCommit), "builtin catalog should pin the exact CC Switch source commit");
assert.ok(rustCatalogFile.includes(sourceLabel), "builtin catalog should identify CC Switch as the pricing source");
assert.ok(rustCatalogSource.includes("2026-08-25"), "builtin catalog should record the CC Switch source check date");
assert.ok(!rustCatalogSource.includes("per_1m_tokens"), "builtin catalog should not use the old per_1m_tokens unit");
assert.equal(
  [...rustCatalogFile.matchAll(new RegExp(ccSwitchCommit, "g"))].length,
  2,
  "generated catalog should pin and attribute the CC Switch source commit",
);

assert.ok(rustCatalogFile.includes('unit: "M".to_string()'), "Rust catalog adapter should use the short M unit label");
assert.ok(!rustCatalogFile.includes("per_1m_tokens"), "Rust catalog should not use the old per_1m_tokens unit");

for (const row of [
  ["openai", "gpt-5.5", 5, 30],
  ["openai", "gpt-5.4", 2.5, 15],
  ["openai", "gpt-5.4-mini", 0.75, 4.5],
  ["openai", "gpt-5.4-nano", 0.2, 1.25],
  ["anthropic", "claude-opus-5", 5, 25],
  ["anthropic", "claude-fable-5", 10, 50],
  ["google", "gemini-2.5-pro", 1.25, 10],
  ["google", "gemini-3.6-flash", 1.5, 7.5],
  ["openai", "gpt-5.6-luna", 0.2, 1.2],
  ["openai", "gpt-5.6-terra", 2, 12],
  ["deepseek", "deepseek-chat", 0.44, 1.32],
  ["alibaba", "qwen3.8-max", 2, 6],
]) {
  assertCatalogRow(rustRows, "Rust builtin seed", row);
}

assertCatalogFields(rustRows, "openai", "gpt-5.6-terra", {
  cacheCreationPrice: 2.5,
  cacheReadPrice: 0.2,
});
assertCatalogFields(rustRows, "anthropic", "claude-opus-5", {
  cacheCreationPrice: 6.25,
  cacheReadPrice: 0.5,
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
      cacheCreationPrice: rustOption(block, "cache_creation_price"),
      cacheReadPrice: rustOption(block, "cache_read_price"),
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

function extractBetween(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start);
  assert.notEqual(start, -1, `missing start marker ${startMarker}`);
  assert.notEqual(end, -1, `missing end marker ${endMarker}`);
  return source.slice(start, end);
}
