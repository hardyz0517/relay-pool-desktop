import { invoke } from "@tauri-apps/api/core";
import type { BalanceSnapshot, PricingRule } from "@/lib/types/economics";

export function listPricingRules() {
  return invoke<PricingRule[]>("list_pricing_rules");
}

export function upsertPricingRule(input: unknown) {
  return invoke<PricingRule>("upsert_pricing_rule", { input });
}

export function deletePricingRule(id: string) {
  return invoke<void>("delete_pricing_rule", { id });
}

export function listBalanceSnapshots() {
  return invoke<BalanceSnapshot[]>("list_balance_snapshots");
}

export function upsertBalanceSnapshot(input: unknown) {
  return invoke<BalanceSnapshot>("upsert_balance_snapshot", { input });
}
