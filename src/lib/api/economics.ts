import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  RequestKind,
  UpsertBalanceSnapshotInput,
  UpsertModelBasePriceInput,
  UpsertPricingRuleInput,
} from "@/lib/types/economics";

export function listPricingRules() {
  return getActiveBackendClient().economics.listPricingRules();
}

export function upsertPricingRule(input: UpsertPricingRuleInput) {
  return getActiveBackendClient().economics.upsertPricingRule(input);
}

export function deletePricingRule(id: string) {
  return getActiveBackendClient().economics.deletePricingRule(id);
}

export function resolveStationKeyPricingContext(
  stationKeyId: string,
  requestedModel: string,
  requestKind: RequestKind = "text",
) {
  return getActiveBackendClient().economics.resolveStationKeyPricingContext(
    stationKeyId,
    requestedModel,
    requestKind,
  );
}

export function listModelBasePrices() {
  return getActiveBackendClient().economics.listModelBasePrices();
}

export function upsertModelBasePrice(input: UpsertModelBasePriceInput) {
  return getActiveBackendClient().economics.upsertModelBasePrice(input);
}

export function resetModelBasePricesToBuiltins() {
  return getActiveBackendClient().economics.resetModelBasePricesToBuiltins();
}

export function listBalanceSnapshots() {
  return getActiveBackendClient().economics.listBalanceSnapshots();
}

export function listCurrentStationBalanceSnapshots() {
  return getActiveBackendClient().economics.listCurrentStationBalanceSnapshots();
}

export function listBalanceSnapshotsForStation(stationId: string) {
  return getActiveBackendClient().economics.listBalanceSnapshotsForStation(stationId);
}

export function upsertBalanceSnapshot(input: UpsertBalanceSnapshotInput) {
  return getActiveBackendClient().economics.upsertBalanceSnapshot(input);
}
