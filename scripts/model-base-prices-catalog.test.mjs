import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const sub2ApiCommit = "e316ebf52838a89d57fc790981cce7520f819ac8";
const expectedBuiltinCount = 196;
const sourceLabel = "Sub2API model pricing catalog";

const rustFile = await readFile("src-tauri/src/services/database.rs", "utf8");
const tsFile = await readFile("src/lib/api/economics.ts", "utf8");
const rustCatalogSource = extractBetween(
  rustFile,
  "const BUILTIN_MODEL_BASE_PRICE_CHECKED_AT",
  "fn seed_builtin_model_base_prices",
);
const rustSeedSource = extractBetween(
  rustFile,
  "fn seed_builtin_model_base_prices",
  "fn seed_builtin_channel_monitor_templates_in_connection",
);
const tsSource = extractBetween(
  tsFile,
  "function builtinModelBasePrices()",
  "function coercePricingRule",
);

const rustRows = parseRustRows(rustCatalogSource);
const tsRows = parseTsRows(tsSource);

assert.equal(
  rustRows.length,
  expectedBuiltinCount,
  "Rust builtin model base prices should mirror the Sub2API pricing catalog row count",
);
assert.equal(
  tsRows.length,
  expectedBuiltinCount,
  "TypeScript fallback model base prices should mirror the Sub2API pricing catalog row count",
);

assert.ok(
  rustSeedSource.includes("DELETE FROM model_base_prices WHERE built_in = 1"),
  "resetting builtins should remove stale old builtin rows before inserting the Sub2API catalog",
);

for (const source of [rustCatalogSource, tsSource]) {
  assert.ok(source.includes(sub2ApiCommit), "builtin catalog should pin the exact Sub2API source commit");
  assert.ok(source.includes(sourceLabel), "builtin catalog should identify Sub2API as the pricing source");
  assert.ok(source.includes("2026-07-12"), "builtin catalog should record the Sub2API source check date");
  assert.ok(!source.includes("per_1m_tokens"), "builtin catalog should not use the old per_1m_tokens unit");
}

assert.ok(rustSeedSource.includes("'M'"), "Rust seed should use the short M unit label");
assert.ok(!rustSeedSource.includes("per_1m_tokens"), "Rust seed should not use the old per_1m_tokens unit");
assert.ok(tsSource.includes('unit: "M"'), "TypeScript fallback should use the short M unit label");

for (const row of [
  ["openai", "gpt-5.5", 5, 30],
  ["openai", "gpt-5.4", 2.5, 15],
  ["openai", "gpt-5.4-mini", 0.75, 4.5],
  ["openai", "gpt-5.4-nano", 0.2, 1.25],
  ["anthropic", "claude-opus-4-5", 5, 25],
  ["anthropic", "claude-sonnet-4-5", 3, 15],
  ["anthropic", "claude-haiku-4-5", 1, 5],
  ["google", "gemini-2.5-pro", 1.25, 10],
  ["openai", "gpt-image-1", 5, 40],
  ["deepseek", "deepseek-chat", 0.28, 0.42],
]) {
  assertCatalogRow(rustRows, "Rust builtin seed", row);
  assertCatalogRow(tsRows, "TypeScript memory fallback", row);
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

function parseTsRows(source) {
  return [...source.matchAll(
    /\[\s*"([^"]+)",\s*"([^"]+)",\s*"([^"]+)",\s*([0-9.]+),\s*([0-9.]+),\s*"[^"]+",\s*"[^"]+",\s*"[^"]+"\s*\]/g,
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
