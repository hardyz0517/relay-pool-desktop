import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const rustFile = await readFile("src-tauri/src/services/database.rs", "utf8");
const tsFile = await readFile("src/lib/api/economics.ts", "utf8");
const rustSource = extractBetween(rustFile, "const BUILTIN_MODEL_BASE_PRICES", "fn seed_builtin_model_base_prices");
const tsSource = extractBetween(tsFile, "function builtinModelBasePrices()", "function coercePricingRule");

const expectedRows = [
  ["openai", "gpt-5.5", 2.5, 15],
  ["openai", "gpt-5.4", 1.25, 7.5],
  ["openai", "gpt-5.4-mini", 0.375, 2.25],
  ["anthropic", "claude-fable-5", 10, 50],
  ["anthropic", "claude-opus-4-8", 5, 25],
  ["anthropic", "claude-opus-4-7", 5, 25],
  ["anthropic", "claude-sonnet-5", 2, 10],
  ["anthropic", "claude-sonnet-4-6", 3, 15],
  ["anthropic", "claude-haiku-4-5", 1, 5],
  ["google", "gemini-3.1-pro-preview", 2, 12],
  ["google", "gemini-3-flash-preview", 0.5, 3],
  ["google", "gemini-2.5-pro", 1.25, 10],
  ["google", "gemini-2.5-flash", 0.3, 2.5],
  ["google", "gemini-2.5-flash-lite", 0.1, 0.4],
  ["xai", "grok-build-0.1", 1, 2],
  ["xai", "grok-4.3", 1.25, 2.5],
  ["xai", "grok-4.20-multi-agent-0309", 1.25, 2.5],
  ["xai", "grok-4.20-0309-reasoning", 1.25, 2.5],
  ["xai", "grok-4.20-0309-non-reasoning", 1.25, 2.5],
];

for (const [provider, model, inputPrice, outputPrice] of expectedRows) {
  assertCatalogRow(rustSource, "Rust builtin seed", provider, model, inputPrice, outputPrice);
  assertCatalogRow(tsSource, "TypeScript memory fallback", provider, model, inputPrice, outputPrice);
}

for (const retiredModel of ["gpt-5", "gpt-5-mini", "gpt-5-nano", "grok-build"]) {
  assert.ok(!rustSource.includes(`model: "${retiredModel}"`), `Rust seed should not retain stale ${retiredModel}`);
  assert.ok(!tsSource.includes(`"${retiredModel}"`), `TypeScript fallback should not retain stale ${retiredModel}`);
}

function assertCatalogRow(source, label, provider, model, inputPrice, outputPrice) {
  assert.ok(source.includes(`"${provider}"`), `${label} should include provider ${provider}`);
  assert.ok(source.includes(`"${model}"`), `${label} should include model ${model}`);
  assert.ok(
    source.includes(numberLiteral(inputPrice)) || source.includes(String(inputPrice)),
    `${label} should include input price ${inputPrice} for ${model}`,
  );
  assert.ok(
    source.includes(numberLiteral(outputPrice)) || source.includes(String(outputPrice)),
    `${label} should include output price ${outputPrice} for ${model}`,
  );
}

function numberLiteral(value) {
  return Number.isInteger(value) ? `${value}.0` : String(value);
}

function extractBetween(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start);
  assert.notEqual(start, -1, `missing start marker ${startMarker}`);
  assert.notEqual(end, -1, `missing end marker ${endMarker}`);
  return source.slice(start, end);
}
