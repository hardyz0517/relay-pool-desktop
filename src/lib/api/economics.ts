import { invoke } from "@tauri-apps/api/core";
import { mockPricingRows } from "@/lib/mock/pricing";
import type { BalanceSnapshot, PricingRule } from "@/lib/types/economics";

let memoryPricingRules: PricingRule[] | null = null;
let memoryBalanceSnapshots: BalanceSnapshot[] | null = null;

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
