import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const sub2ApiCommit = "b22f73e725236790f97d89bf0c3b908a48e591d5";
const expectedBuiltinCount = 198;
const sourceLabel = "Sub2API model pricing catalog";

const rustCatalogFile = await readFile("src-tauri/src/services/pricing_catalog.rs", "utf8");
const rustStoreFile = await readFile("src-tauri/src/persistence/stores/pricing_store.rs", "utf8");
const rustCatalogSource = extractBetween(
  rustCatalogFile,
  "pub const BUILTIN_MODEL_BASE_PRICE_CHECKED_AT",
  "pub struct StaticBuiltinModelBasePriceCatalog",
);
const rustSeedSource = extractBetween(
  rustStoreFile,
  "pub async fn reset_model_base_prices_to_builtins",
  "pub async fn ensure_builtin_model_base_prices",
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
assert.ok(rustCatalogSource.includes("2026-08-01"), "builtin catalog should record the Sub2API source check date");
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
  ["openai", "gpt-image-1", 5, 40],
  ["openai", "codex-auto-review", 0.2, 1.2],
  ["openai", "gpt-5.6-luna", 0.2, 1.2],
  ["openai", "gpt-5.6-terra", 2, 12],
  ["deepseek", "deepseek-chat", 0.28, 0.42],
]) {
  assertCatalogRow(rustRows, "Rust builtin seed", row);
}

function assertCatalogRow(rows, label, [provider, model, inputPrice, outputPrice]) {
  const row = rows.find((candidate) => candidate.provider === provider && candidate.model === model);
  assert.ok(row, `${label} should include ${provider}/${model}`);
  assert.equal(row.inputPrice, inputPrice, `${label} input price for ${model}`);
  assert.equal(row.outputPrice, outputPrice, `${label} output price for ${model}`);
}

function parseRustRows(source) {
  return [...source.matchAll(
    /BuiltinModelBasePrice\s*\{\s*id:\s*"([^"]+)",\s*provider:\s*"([^"]+)",\s*model:\s*"([^"]+)",\s*input_price:\s*([0-9.]+),\s*output_price:\s*([0-9.]+),/g,
  )].map((match) => ({
    id: match[1],
    provider: match[2],
    model: match[3],
    inputPrice: Number(match[4]),
    outputPrice: Number(match[5]),
  }));
}

function extractBetween(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start);
  assert.notEqual(start, -1, `missing start marker ${startMarker}`);
  assert.notEqual(end, -1, `missing end marker ${endMarker}`);
  return source.slice(start, end);
}
