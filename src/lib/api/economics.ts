import { invoke } from "@tauri-apps/api/core";
import { mockPricingRows } from "@/lib/mock/pricing";
import type { BalanceSnapshot, ModelBasePrice, PricingRule } from "@/lib/types/economics";

let memoryPricingRules: PricingRule[] | null = null;
let memoryBalanceSnapshots: BalanceSnapshot[] | null = null;
let memoryModelBasePrices: ModelBasePrice[] | null = null;

export function listPricingRules() {
  return invoke<PricingRule[]>("list_pricing_rules").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return ensureMemoryPricingRules();
    }
    throw error;
  });
}

export function upsertPricingRule(input: unknown) {
  return invoke<PricingRule>("upsert_pricing_rule", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const nextRule = coercePricingRule(input);
      memoryPricingRules = [
        nextRule,
        ...ensureMemoryPricingRules().filter((rule) => rule.id !== nextRule.id),
      ];
      return nextRule;
    }
    throw error;
  });
}

export function deletePricingRule(id: string) {
  return invoke<void>("delete_pricing_rule", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      memoryPricingRules = ensureMemoryPricingRules().filter((rule) => rule.id !== id);
      return;
    }
    throw error;
  });
}

export function listModelBasePrices() {
  return invoke<ModelBasePrice[]>("list_model_base_prices").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return ensureMemoryModelBasePrices();
    }
    throw error;
  });
}

export function upsertModelBasePrice(input: unknown) {
  return invoke<ModelBasePrice>("upsert_model_base_price", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const nextPrice = coerceModelBasePrice(input);
      memoryModelBasePrices = [
        nextPrice,
        ...ensureMemoryModelBasePrices().filter((price) => price.id !== nextPrice.id),
      ];
      return nextPrice;
    }
    throw error;
  });
}

export function resetModelBasePricesToBuiltins() {
  return invoke<ModelBasePrice[]>("reset_model_base_prices_to_builtins").catch((error) => {
    if (isInvokeUnavailable(error)) {
      memoryModelBasePrices = builtinModelBasePrices();
      return memoryModelBasePrices;
    }
    throw error;
  });
}

export function listBalanceSnapshots() {
  return invoke<BalanceSnapshot[]>("list_balance_snapshots").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return ensureMemoryBalanceSnapshots();
    }
    throw error;
  });
}

export function listBalanceSnapshotsForStation(stationId: string) {
  return invoke<BalanceSnapshot[]>("list_balance_snapshots_for_station", { stationId }).catch((error) => {
    if (isCommandNotFound(error)) {
      return listBalanceSnapshots().then((snapshots) =>
        snapshots.filter((snapshot) => snapshot.stationId === stationId),
      );
    }
    if (isInvokeUnavailable(error)) {
      return ensureMemoryBalanceSnapshots().filter((snapshot) => snapshot.stationId === stationId);
    }
    throw error;
  });
}

export function upsertBalanceSnapshot(input: unknown) {
  return invoke<BalanceSnapshot>("upsert_balance_snapshot", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      const nextSnapshot = coerceBalanceSnapshot(input);
      memoryBalanceSnapshots = [
        nextSnapshot,
        ...ensureMemoryBalanceSnapshots().filter((snapshot) => snapshot.id !== nextSnapshot.id),
      ];
      return nextSnapshot;
    }
    throw error;
  });
}

function ensureMemoryPricingRules() {
  if (memoryPricingRules) {
    return memoryPricingRules;
  }

  const now = new Date().toISOString();
  memoryPricingRules = mockPricingRows.flatMap((row) =>
    row.stationPrices.map((price, index) => ({
      id: `mock-price-${row.model}-${index}`,
      stationId: stationIdFromName(price.stationName),
      stationKeyId: null,
      groupBindingId: null,
      groupName: price.groupRatio,
      tierLabel: price.modelRatio,
      model: row.model,
      inputPrice: price.inputCnyPer1M,
      outputPrice: price.outputCnyPer1M,
      fixedPrice: null,
      rateMultiplier: null,
      currency: "CNY",
      unit: "1M tokens",
      priceType: "token",
      basePriceSource: "mock_matrix",
      normalizationStatus: "complete",
      source: "mock",
      confidence: 0.8,
      enabled: true,
      note: null,
      collectedAt: now,
      validFrom: null,
      validUntil: null,
      createdAt: now,
      updatedAt: now,
    })),
  );

  return memoryPricingRules;
}

function ensureMemoryBalanceSnapshots() {
  if (memoryBalanceSnapshots) {
    return memoryBalanceSnapshots;
  }

  const now = new Date().toISOString();
  memoryBalanceSnapshots = [
    {
      id: "mock-balance-orchid",
      stationId: "st-orchid",
      stationKeyId: null,
      scope: "station",
      value: 86.42,
      currency: "CNY",
      creditUnit: null,
      usedValue: null,
      totalValue: null,
      lowBalanceThreshold: 15,
      status: "normal",
      source: "mock",
      confidence: 0.8,
      collectedAt: now,
      createdAt: now,
      updatedAt: now,
    },
    {
      id: "mock-balance-lantern",
      stationId: "st-lantern",
      stationKeyId: null,
      scope: "station",
      value: 12.35,
      currency: "CNY",
      creditUnit: null,
      usedValue: null,
      totalValue: null,
      lowBalanceThreshold: 15,
      status: "low",
      source: "mock",
      confidence: 0.8,
      collectedAt: now,
      createdAt: now,
      updatedAt: now,
    },
  ];

  return memoryBalanceSnapshots;
}

function ensureMemoryModelBasePrices() {
  if (memoryModelBasePrices) {
    return memoryModelBasePrices;
  }

  memoryModelBasePrices = builtinModelBasePrices();
  return memoryModelBasePrices;
}

function builtinModelBasePrices(): ModelBasePrice[] {
  const now = new Date().toISOString();
  const rows = [
    ["builtin-openai-gpt-5-5", "openai", "gpt-5.5", 2.5, 15, "https://developers.openai.com/api/docs/pricing", "OpenAI API pricing", "Standard text token price per 1M tokens for short context."],
    ["builtin-openai-gpt-5-4", "openai", "gpt-5.4", 1.25, 7.5, "https://developers.openai.com/api/docs/pricing", "OpenAI API pricing", "Standard text token price per 1M tokens for short context."],
    ["builtin-openai-gpt-5-4-mini", "openai", "gpt-5.4-mini", 0.375, 2.25, "https://developers.openai.com/api/docs/pricing", "OpenAI API pricing", "Standard text token price per 1M tokens."],
    ["builtin-anthropic-claude-fable-5", "anthropic", "claude-fable-5", 10, 50, "https://docs.anthropic.com/en/docs/about-claude/pricing", "Anthropic Claude pricing", "Standard text token price per 1M tokens."],
    ["builtin-anthropic-claude-opus-4-8", "anthropic", "claude-opus-4-8", 5, 25, "https://docs.anthropic.com/en/docs/about-claude/pricing", "Anthropic Claude pricing", "Standard text token price per 1M tokens."],
    ["builtin-anthropic-claude-opus-4-7", "anthropic", "claude-opus-4-7", 5, 25, "https://docs.anthropic.com/en/docs/about-claude/pricing", "Anthropic Claude pricing", "Standard text token price per 1M tokens."],
    ["builtin-anthropic-claude-sonnet-5", "anthropic", "claude-sonnet-5", 2, 10, "https://docs.anthropic.com/en/docs/about-claude/pricing", "Anthropic Claude pricing", "Promotional text token price per 1M tokens through 2026-08-31."],
    ["builtin-anthropic-claude-sonnet-4-6", "anthropic", "claude-sonnet-4-6", 3, 15, "https://docs.anthropic.com/en/docs/about-claude/pricing", "Anthropic Claude pricing", "Standard text token price per 1M tokens."],
    ["builtin-anthropic-claude-haiku-4-5", "anthropic", "claude-haiku-4-5", 1, 5, "https://docs.anthropic.com/en/docs/about-claude/pricing", "Anthropic Claude pricing", "Standard text token price per 1M tokens."],
    ["builtin-google-gemini-3-1-pro-preview", "google", "gemini-3.1-pro-preview", 2, 12, "https://ai.google.dev/gemini-api/docs/pricing", "Gemini API pricing", "Standard text token price per 1M tokens for prompts at or below 200k tokens."],
    ["builtin-google-gemini-3-flash-preview", "google", "gemini-3-flash-preview", 0.5, 3, "https://ai.google.dev/gemini-api/docs/pricing", "Gemini API pricing", "Standard text token price per 1M tokens."],
    ["builtin-google-gemini-2-5-pro", "google", "gemini-2.5-pro", 1.25, 10, "https://ai.google.dev/gemini-api/docs/pricing", "Gemini API pricing", "Standard text token price per 1M tokens for prompts at or below 200k tokens."],
    ["builtin-google-gemini-2-5-flash", "google", "gemini-2.5-flash", 0.3, 2.5, "https://ai.google.dev/gemini-api/docs/pricing", "Gemini API pricing", "Standard text token price per 1M tokens."],
    ["builtin-google-gemini-2-5-flash-lite", "google", "gemini-2.5-flash-lite", 0.1, 0.4, "https://ai.google.dev/gemini-api/docs/pricing", "Gemini API pricing", "Standard text token price per 1M tokens."],
    ["builtin-xai-grok-build-0-1", "xai", "grok-build-0.1", 1, 2, "https://docs.x.ai/docs/models/grok-build", "xAI Grok Build model card", "Standard text token price per 1M tokens."],
    ["builtin-xai-grok-4-3", "xai", "grok-4.3", 1.25, 2.5, "https://docs.x.ai/developers/pricing", "xAI API pricing", "Standard text token price per 1M tokens."],
    ["builtin-xai-grok-4-20-multi-agent-0309", "xai", "grok-4.20-multi-agent-0309", 1.25, 2.5, "https://docs.x.ai/developers/pricing", "xAI API pricing", "Standard text token price per 1M tokens."],
    ["builtin-xai-grok-4-20-0309-reasoning", "xai", "grok-4.20-0309-reasoning", 1.25, 2.5, "https://docs.x.ai/developers/pricing", "xAI API pricing", "Standard text token price per 1M tokens."],
    ["builtin-xai-grok-4-20-0309-non-reasoning", "xai", "grok-4.20-0309-non-reasoning", 1.25, 2.5, "https://docs.x.ai/developers/pricing", "xAI API pricing", "Standard text token price per 1M tokens."],
  ] as const;

  return rows.map(([id, provider, model, inputPrice, outputPrice, sourceUrl, sourceLabel, note]) => ({
    id,
    provider,
    model,
    inputPrice,
    outputPrice,
    currency: "USD",
    unit: "per_1m_tokens",
    sourceUrl,
    sourceLabel,
    sourceCheckedAt: "2026-07-08",
    enabled: true,
    builtIn: true,
    note,
    createdAt: now,
    updatedAt: now,
  }));
}

function coercePricingRule(input: unknown): PricingRule {
  const now = new Date().toISOString();
  const candidate = typeof input === "object" && input ? (input as Partial<PricingRule>) : {};
  return {
    id: candidate.id ?? `mock-price-${Date.now()}`,
    stationId: candidate.stationId ?? "st-orchid",
    stationKeyId: candidate.stationKeyId ?? null,
    groupBindingId: candidate.groupBindingId ?? null,
    groupName: candidate.groupName ?? null,
    tierLabel: candidate.tierLabel ?? null,
    model: candidate.model ?? "unknown-model",
    inputPrice: candidate.inputPrice ?? null,
    outputPrice: candidate.outputPrice ?? null,
    fixedPrice: candidate.fixedPrice ?? null,
    rateMultiplier: candidate.rateMultiplier ?? null,
    currency: candidate.currency ?? "CNY",
    unit: candidate.unit ?? "1M tokens",
    priceType: candidate.priceType ?? "token",
    basePriceSource: candidate.basePriceSource ?? null,
    normalizationStatus: candidate.normalizationStatus ?? "manual",
    source: candidate.source ?? "mock",
    confidence: candidate.confidence ?? 0.8,
    enabled: candidate.enabled ?? true,
    note: candidate.note ?? null,
    collectedAt: candidate.collectedAt ?? now,
    validFrom: candidate.validFrom ?? null,
    validUntil: candidate.validUntil ?? null,
    createdAt: candidate.createdAt ?? now,
    updatedAt: now,
  };
}

function coerceModelBasePrice(input: unknown): ModelBasePrice {
  const now = new Date().toISOString();
  const candidate = typeof input === "object" && input ? (input as Partial<ModelBasePrice>) : {};
  return {
    id: candidate.id ?? `mock-model-base-price-${Date.now()}`,
    provider: candidate.provider ?? "custom",
    model: candidate.model ?? "unknown-model",
    inputPrice: candidate.inputPrice ?? null,
    outputPrice: candidate.outputPrice ?? null,
    currency: candidate.currency ?? "USD",
    unit: candidate.unit ?? "per_1m_tokens",
    sourceUrl: candidate.sourceUrl ?? "manual",
    sourceLabel: candidate.sourceLabel ?? "Manual",
    sourceCheckedAt: candidate.sourceCheckedAt ?? now.slice(0, 10),
    enabled: candidate.enabled ?? true,
    builtIn: candidate.builtIn ?? false,
    note: candidate.note ?? null,
    createdAt: candidate.createdAt ?? now,
    updatedAt: now,
  };
}

function coerceBalanceSnapshot(input: unknown): BalanceSnapshot {
  const now = new Date().toISOString();
  const candidate = typeof input === "object" && input ? (input as Partial<BalanceSnapshot>) : {};
  return {
    id: candidate.id ?? `mock-balance-${Date.now()}`,
    stationId: candidate.stationId ?? "st-orchid",
    stationKeyId: candidate.stationKeyId ?? null,
    scope: candidate.scope ?? "station",
    value: candidate.value ?? null,
    currency: candidate.currency ?? "CNY",
    creditUnit: candidate.creditUnit ?? null,
    usedValue: candidate.usedValue ?? null,
    totalValue: candidate.totalValue ?? null,
    lowBalanceThreshold: candidate.lowBalanceThreshold ?? null,
    status: candidate.status ?? "unknown",
    source: candidate.source ?? "mock",
    confidence: candidate.confidence ?? 0.8,
    collectedAt: candidate.collectedAt ?? now,
    createdAt: candidate.createdAt ?? now,
    updatedAt: now,
  };
}

function stationIdFromName(name: string) {
  if (name.includes("Lantern")) {
    return "st-lantern";
  }
  if (name.includes("Harbor")) {
    return "st-harbor";
  }
  if (name.includes("Archive")) {
    return "st-archive";
  }
  return "st-orchid";
}

function isInvokeUnavailable(error: unknown) {
  return /invoke|__TAURI__/i.test(getErrorMessage(error));
}

function isCommandNotFound(error: unknown) {
  return /command .* not found/i.test(getErrorMessage(error));
}

function getErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
