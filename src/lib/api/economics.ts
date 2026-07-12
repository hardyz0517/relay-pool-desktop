import { invoke } from "@tauri-apps/api/core";
import { mockPricingRows } from "@/lib/mock/pricing";
import type {
  BalanceSnapshot,
  ModelBasePrice,
  PricingRule,
  RequestKind,
  ResolvedPricingContext,
} from "@/lib/types/economics";

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

export function resolveStationKeyPricingContext(
  stationKeyId: string,
  requestedModel: string,
  requestKind: RequestKind = "text",
) {
  return invoke<ResolvedPricingContext>("resolve_station_key_pricing_context", {
    stationKeyId,
    requestedModel,
    requestKind,
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
      todayRequestCount: 42,
      totalRequestCount: 1280,
      todayConsumption: 1.25,
      totalConsumption: 38.4,
      todayTokenCount: 56000,
      totalTokenCount: 890000,
      todayInputTokenCount: 47200,
      todayOutputTokenCount: 8800,
      totalInputTokenCount: 745000,
      totalOutputTokenCount: 145000,
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
      todayRequestCount: 18,
      totalRequestCount: 640,
      todayConsumption: 0.45,
      totalConsumption: 12.8,
      todayTokenCount: 24000,
      totalTokenCount: 320000,
      todayInputTokenCount: 19600,
      todayOutputTokenCount: 4400,
      totalInputTokenCount: 264000,
      totalOutputTokenCount: 56000,
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
    ["builtin-anthropic-claude-3-7-sonnet-20250219", "anthropic", "claude-3-7-sonnet-20250219", 3.0, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-3-haiku-20240307", "anthropic", "claude-3-haiku-20240307", 0.25, 1.25, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-3-opus-20240229", "anthropic", "claude-3-opus-20240229", 15.0, 75.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-4-opus-20250514", "anthropic", "claude-4-opus-20250514", 15.0, 75.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-4-sonnet-20250514", "anthropic", "claude-4-sonnet-20250514", 3.0, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-haiku-4-5", "anthropic", "claude-haiku-4-5", 1.0, 5.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-haiku-4-5-20251001", "anthropic", "claude-haiku-4-5-20251001", 1.0, 5.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-1", "anthropic", "claude-opus-4-1", 15.0, 75.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-1-20250805", "anthropic", "claude-opus-4-1-20250805", 15.0, 75.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-20250514", "anthropic", "claude-opus-4-20250514", 15.0, 75.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-5", "anthropic", "claude-opus-4-5", 5.0, 25.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-5-20251101", "anthropic", "claude-opus-4-5-20251101", 5.0, 25.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-6", "anthropic", "claude-opus-4-6", 5.0, 25.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-6-20260205", "anthropic", "claude-opus-4-6-20260205", 5.0, 25.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-6-thinking", "anthropic", "claude-opus-4-6-thinking", 5.0, 25.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-7", "anthropic", "claude-opus-4-7", 5.0, 25.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-7-20260416", "anthropic", "claude-opus-4-7-20260416", 5.0, 25.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-opus-4-8", "anthropic", "claude-opus-4-8", 5.0, 25.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-sonnet-4-20250514", "anthropic", "claude-sonnet-4-20250514", 3.0, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-sonnet-4-5", "anthropic", "claude-sonnet-4-5", 3.0, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-sonnet-4-5-20250929", "anthropic", "claude-sonnet-4-5-20250929", 3.0, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-anthropic-claude-sonnet-4-6", "anthropic", "claude-sonnet-4-6", 3.0, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-bedrock-claude-sonnet-4-5-20250929-v1-0", "bedrock", "claude-sonnet-4-5-20250929-v1:0", 3.0, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-deepseek-deepseek-chat", "deepseek", "deepseek-chat", 0.28, 0.42, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-deepseek-deepseek-reasoner", "deepseek", "deepseek-reasoner", 0.28, 0.42, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-0-flash", "google", "gemini-2.0-flash", 0.1, 0.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-0-flash-001", "google", "gemini-2.0-flash-001", 0.15, 0.6, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-0-flash-exp-image-generation", "google", "gemini-2.0-flash-exp-image-generation", 0.0, 0.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-0-flash-lite", "google", "gemini-2.0-flash-lite", 0.075, 0.3, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-0-flash-lite-001", "google", "gemini-2.0-flash-lite-001", 0.075, 0.3, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-computer-use-preview-10-2025", "google", "gemini-2.5-computer-use-preview-10-2025", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash", "google", "gemini-2.5-flash", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash-image", "google", "gemini-2.5-flash-image", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash-lite", "google", "gemini-2.5-flash-lite", 0.1, 0.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash-lite-preview-06-17", "google", "gemini-2.5-flash-lite-preview-06-17", 0.1, 0.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash-lite-preview-09-2025", "google", "gemini-2.5-flash-lite-preview-09-2025", 0.1, 0.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash-native-audio-latest", "google", "gemini-2.5-flash-native-audio-latest", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash-native-audio-preview-09-2025", "google", "gemini-2.5-flash-native-audio-preview-09-2025", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash-native-audio-preview-12-2025", "google", "gemini-2.5-flash-native-audio-preview-12-2025", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash-preview-09-2025", "google", "gemini-2.5-flash-preview-09-2025", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-flash-preview-tts", "google", "gemini-2.5-flash-preview-tts", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM audio_speech pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-pro", "google", "gemini-2.5-pro", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-2-5-pro-preview-tts", "google", "gemini-2.5-pro-preview-tts", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-flash", "google", "gemini-3-flash", 0.5, 3.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-flash-preview", "google", "gemini-3-flash-preview", 0.5, 3.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-pro-image", "google", "gemini-3-pro-image", 2.0, 12.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-pro-image-preview", "google", "gemini-3-pro-image-preview", 2.0, 12.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-pro-preview", "google", "gemini-3-pro-preview", 2.0, 12.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-flash-image", "google", "gemini-3.1-flash-image", 0.5, 3.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-flash-image-preview", "google", "gemini-3.1-flash-image-preview", 0.5, 3.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-flash-lite", "google", "gemini-3.1-flash-lite", 0.25, 1.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-flash-lite-image", "google", "gemini-3.1-flash-lite-image", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-flash-lite-preview", "google", "gemini-3.1-flash-lite-preview", 0.25, 1.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-flash-live-preview", "google", "gemini-3.1-flash-live-preview", 0.75, 4.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-pro-high", "google", "gemini-3.1-pro-high", 2.0, 12.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-pro-low", "google", "gemini-3.1-pro-low", 2.0, 12.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-pro-preview", "google", "gemini-3.1-pro-preview", 2.0, 12.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-1-pro-preview-customtools", "google", "gemini-3.1-pro-preview-customtools", 2.0, 12.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-3-5-flash", "google", "gemini-3.5-flash", 1.5, 9.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-embedding-001", "google", "gemini-embedding-001", 0.15, 0.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM embedding pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-embedding-2", "google", "gemini-embedding-2", 0.2, 0.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM embedding pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-embedding-2-preview", "google", "gemini-embedding-2-preview", 0.2, 0.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM embedding pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-exp-1206", "google", "gemini-exp-1206", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-flash-experimental", "google", "gemini-flash-experimental", 0.0, 0.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM embedding pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-flash-latest", "google", "gemini-flash-latest", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-flash-lite-latest", "google", "gemini-flash-lite-latest", 0.1, 0.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-live-2-5-flash-preview-native-audio-09-2025", "google", "gemini-live-2.5-flash-preview-native-audio-09-2025", 0.3, 2.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM realtime pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-pro-latest", "google", "gemini-pro-latest", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-google-gemini-robotics-er-1-5-preview", "google", "gemini-robotics-er-1.5-preview", 0.3, 2.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-codex-auto-review", "openai", "codex-auto-review", 5.0, 30.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-3-5-turbo", "openai", "gpt-3.5-turbo", 0.5, 1.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-3-5-turbo-0125", "openai", "gpt-3.5-turbo-0125", 0.5, 1.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-3-5-turbo-1106", "openai", "gpt-3.5-turbo-1106", 1.0, 2.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-3-5-turbo-16k", "openai", "gpt-3.5-turbo-16k", 3.0, 4.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4", "openai", "gpt-4", 30.0, 60.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-0125-preview", "openai", "gpt-4-0125-preview", 10.0, 30.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-0314", "openai", "gpt-4-0314", 30.0, 60.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-0613", "openai", "gpt-4-0613", 30.0, 60.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-1106-preview", "openai", "gpt-4-1106-preview", 10.0, 30.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-turbo", "openai", "gpt-4-turbo", 10.0, 30.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-turbo-2024-04-09", "openai", "gpt-4-turbo-2024-04-09", 10.0, 30.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-turbo-preview", "openai", "gpt-4-turbo-preview", 10.0, 30.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-1", "openai", "gpt-4.1", 2.0, 8.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-1-2025-04-14", "openai", "gpt-4.1-2025-04-14", 2.0, 8.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-1-mini", "openai", "gpt-4.1-mini", 0.4, 1.6, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-1-mini-2025-04-14", "openai", "gpt-4.1-mini-2025-04-14", 0.4, 1.6, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-1-nano", "openai", "gpt-4.1-nano", 0.1, 0.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4-1-nano-2025-04-14", "openai", "gpt-4.1-nano-2025-04-14", 0.1, 0.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o", "openai", "gpt-4o", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-2024-05-13", "openai", "gpt-4o-2024-05-13", 5.0, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-2024-08-06", "openai", "gpt-4o-2024-08-06", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-2024-11-20", "openai", "gpt-4o-2024-11-20", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-audio-preview", "openai", "gpt-4o-audio-preview", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-audio-preview-2024-12-17", "openai", "gpt-4o-audio-preview-2024-12-17", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-audio-preview-2025-06-03", "openai", "gpt-4o-audio-preview-2025-06-03", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini", "openai", "gpt-4o-mini", 0.15, 0.6, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-2024-07-18", "openai", "gpt-4o-mini-2024-07-18", 0.15, 0.6, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-audio-preview", "openai", "gpt-4o-mini-audio-preview", 0.15, 0.6, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-audio-preview-2024-12-17", "openai", "gpt-4o-mini-audio-preview-2024-12-17", 0.15, 0.6, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-realtime-preview", "openai", "gpt-4o-mini-realtime-preview", 0.6, 2.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-realtime-preview-2024-12-17", "openai", "gpt-4o-mini-realtime-preview-2024-12-17", 0.6, 2.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-search-preview", "openai", "gpt-4o-mini-search-preview", 0.15, 0.6, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-search-preview-2025-03-11", "openai", "gpt-4o-mini-search-preview-2025-03-11", 0.15, 0.6, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-transcribe", "openai", "gpt-4o-mini-transcribe", 1.25, 5.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-transcribe-2025-03-20", "openai", "gpt-4o-mini-transcribe-2025-03-20", 1.25, 5.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-transcribe-2025-12-15", "openai", "gpt-4o-mini-transcribe-2025-12-15", 1.25, 5.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-tts", "openai", "gpt-4o-mini-tts", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM audio_speech pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-tts-2025-03-20", "openai", "gpt-4o-mini-tts-2025-03-20", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM audio_speech pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-mini-tts-2025-12-15", "openai", "gpt-4o-mini-tts-2025-12-15", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM audio_speech pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-realtime-preview", "openai", "gpt-4o-realtime-preview", 5.0, 20.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-realtime-preview-2024-12-17", "openai", "gpt-4o-realtime-preview-2024-12-17", 5.0, 20.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-realtime-preview-2025-06-03", "openai", "gpt-4o-realtime-preview-2025-06-03", 5.0, 20.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-search-preview", "openai", "gpt-4o-search-preview", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-search-preview-2025-03-11", "openai", "gpt-4o-search-preview-2025-03-11", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-transcribe", "openai", "gpt-4o-transcribe", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-4o-transcribe-diarize", "openai", "gpt-4o-transcribe-diarize", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM audio_transcription pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5", "openai", "gpt-5", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-2025-08-07", "openai", "gpt-5-2025-08-07", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-chat", "openai", "gpt-5-chat", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-chat-latest", "openai", "gpt-5-chat-latest", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-codex", "openai", "gpt-5-codex", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-mini", "openai", "gpt-5-mini", 0.25, 2.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-mini-2025-08-07", "openai", "gpt-5-mini-2025-08-07", 0.25, 2.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-nano", "openai", "gpt-5-nano", 0.05, 0.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-nano-2025-08-07", "openai", "gpt-5-nano-2025-08-07", 0.05, 0.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-pro", "openai", "gpt-5-pro", 15.0, 120.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-pro-2025-10-06", "openai", "gpt-5-pro-2025-10-06", 15.0, 120.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-search-api", "openai", "gpt-5-search-api", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-search-api-2025-10-14", "openai", "gpt-5-search-api-2025-10-14", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-1", "openai", "gpt-5.1", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-1-2025-11-13", "openai", "gpt-5.1-2025-11-13", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-1-chat-latest", "openai", "gpt-5.1-chat-latest", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-1-codex", "openai", "gpt-5.1-codex", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-1-codex-max", "openai", "gpt-5.1-codex-max", 1.25, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-1-codex-mini", "openai", "gpt-5.1-codex-mini", 0.25, 2.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-2", "openai", "gpt-5.2", 1.75, 14.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-2-2025-12-11", "openai", "gpt-5.2-2025-12-11", 1.75, 14.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-2-chat-latest", "openai", "gpt-5.2-chat-latest", 1.75, 14.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-2-codex", "openai", "gpt-5.2-codex", 1.75, 14.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-2-pro", "openai", "gpt-5.2-pro", 21.0, 168.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-2-pro-2025-12-11", "openai", "gpt-5.2-pro-2025-12-11", 21.0, 168.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-3-chat-latest", "openai", "gpt-5.3-chat-latest", 1.75, 14.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-3-codex", "openai", "gpt-5.3-codex", 1.75, 14.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-3-codex-spark", "openai", "gpt-5.3-codex-spark", 1.75, 14.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-4", "openai", "gpt-5.4", 2.5, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-4-2026-03-05", "openai", "gpt-5.4-2026-03-05", 2.5, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-4-mini", "openai", "gpt-5.4-mini", 0.75, 4.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-4-mini-2026-03-17", "openai", "gpt-5.4-mini-2026-03-17", 0.75, 4.5, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-4-nano", "openai", "gpt-5.4-nano", 0.2, 1.25, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-4-nano-2026-03-17", "openai", "gpt-5.4-nano-2026-03-17", 0.2, 1.25, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-4-pro", "openai", "gpt-5.4-pro", 30.0, 180.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-4-pro-2026-03-05", "openai", "gpt-5.4-pro-2026-03-05", 30.0, 180.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-5", "openai", "gpt-5.5", 5.0, 30.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-5-2026-04-23", "openai", "gpt-5.5-2026-04-23", 5.0, 30.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-5-pro", "openai", "gpt-5.5-pro", 30.0, 180.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-5-pro-2026-04-23", "openai", "gpt-5.5-pro-2026-04-23", 30.0, 180.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-6-luna", "openai", "gpt-5.6-luna", 1.0, 6.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-6-sol", "openai", "gpt-5.6-sol", 5.0, 30.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-5-6-terra", "openai", "gpt-5.6-terra", 2.5, 15.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-audio", "openai", "gpt-audio", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-audio-1-5", "openai", "gpt-audio-1.5", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-audio-2025-08-28", "openai", "gpt-audio-2025-08-28", 2.5, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-audio-mini", "openai", "gpt-audio-mini", 0.6, 2.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-audio-mini-2025-10-06", "openai", "gpt-audio-mini-2025-10-06", 0.6, 2.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-audio-mini-2025-12-15", "openai", "gpt-audio-mini-2025-12-15", 0.6, 2.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-image-1", "openai", "gpt-image-1", 5.0, 40.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image-generation pricing: input_cost_per_token and output_cost_per_image_token converted to USD per M."],
    ["builtin-openai-gpt-image-1-mini", "openai", "gpt-image-1-mini", 2.0, 8.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image-generation pricing: input_cost_per_token and output_cost_per_image_token converted to USD per M."],
    ["builtin-openai-gpt-image-1-5", "openai", "gpt-image-1.5", 5.0, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-image-1-5-2025-12-16", "openai", "gpt-image-1.5-2025-12-16", 5.0, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-image-2", "openai", "gpt-image-2", 5.0, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-image-2-2026-04-21", "openai", "gpt-image-2-2026-04-21", 5.0, 10.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM image_generation pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-realtime", "openai", "gpt-realtime", 4.0, 16.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-realtime-1-5", "openai", "gpt-realtime-1.5", 4.0, 16.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-realtime-2", "openai", "gpt-realtime-2", 4.0, 16.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-realtime-2025-08-28", "openai", "gpt-realtime-2025-08-28", 4.0, 16.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-realtime-mini", "openai", "gpt-realtime-mini", 0.6, 2.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-realtime-mini-2025-10-06", "openai", "gpt-realtime-mini-2025-10-06", 0.6, 2.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-gpt-realtime-mini-2025-12-15", "openai", "gpt-realtime-mini-2025-12-15", 0.6, 2.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o1-2024-12-17", "openai", "o1-2024-12-17", 15.0, 60.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o1-pro", "openai", "o1-pro", 150.0, 600.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o1-pro-2025-03-19", "openai", "o1-pro-2025-03-19", 150.0, 600.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o3", "openai", "o3", 2.0, 8.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o3-2025-04-16", "openai", "o3-2025-04-16", 2.0, 8.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o3-deep-research", "openai", "o3-deep-research", 10.0, 40.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o3-deep-research-2025-06-26", "openai", "o3-deep-research-2025-06-26", 10.0, 40.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o3-mini", "openai", "o3-mini", 1.1, 4.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o3-mini-2025-01-31", "openai", "o3-mini-2025-01-31", 1.1, 4.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o3-pro", "openai", "o3-pro", 20.0, 80.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o3-pro-2025-06-10", "openai", "o3-pro-2025-06-10", 20.0, 80.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o4-mini", "openai", "o4-mini", 1.1, 4.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o4-mini-2025-04-16", "openai", "o4-mini-2025-04-16", 1.1, 4.4, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o4-mini-deep-research", "openai", "o4-mini-deep-research", 2.0, 8.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-openai-o4-mini-deep-research-2025-06-26", "openai", "o4-mini-deep-research-2025-06-26", 2.0, 8.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM responses pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-text-completion-openai-gpt-3-5-turbo-instruct", "text-completion-openai", "gpt-3.5-turbo-instruct", 1.5, 2.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM completion pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-text-completion-openai-gpt-3-5-turbo-instruct-0914", "text-completion-openai", "gpt-3.5-turbo-instruct-0914", 1.5, 2.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM completion pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
    ["builtin-volcengine-deepseek-v3-2-251201", "volcengine", "deepseek-v3-2-251201", 0.0, 0.0, "https://github.com/Wei-Shaw/sub2api/blob/e316ebf52838a89d57fc790981cce7520f819ac8/backend/resources/model-pricing/model_prices_and_context_window.json", "Sub2API model pricing catalog", "Sub2API/LiteLLM chat pricing: input_cost_per_token and output_cost_per_token converted to USD per M."],
  ] as const;

  return rows.map(([id, provider, model, inputPrice, outputPrice, sourceUrl, sourceLabel, note]) => ({
    id,
    provider,
    model,
    inputPrice,
    outputPrice,
    currency: "USD",
    unit: "M",
    sourceUrl,
    sourceLabel,
    sourceCheckedAt: "2026-07-12",
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
    unit: candidate.unit ?? "M",
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
    todayRequestCount: candidate.todayRequestCount ?? null,
    totalRequestCount: candidate.totalRequestCount ?? null,
    todayConsumption: candidate.todayConsumption ?? null,
    totalConsumption: candidate.totalConsumption ?? null,
    todayTokenCount: candidate.todayTokenCount ?? null,
    totalTokenCount: candidate.totalTokenCount ?? null,
    todayInputTokenCount: candidate.todayInputTokenCount ?? null,
    todayOutputTokenCount: candidate.todayOutputTokenCount ?? null,
    totalInputTokenCount: candidate.totalInputTokenCount ?? null,
    totalOutputTokenCount: candidate.totalOutputTokenCount ?? null,
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
