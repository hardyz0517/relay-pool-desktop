import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  RequestKind,
  ModelPriceSyncConfig,
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

export function listModelPriceSyncCatalog() {
  return getActiveBackendClient().economics.listModelPriceSyncCatalog();
}

export function upsertModelBasePrice(input: UpsertModelBasePriceInput) {
  return getActiveBackendClient().economics.upsertModelBasePrice(input);
}

export function deleteModelBasePrice(id: string) {
  return getActiveBackendClient().economics.deleteModelBasePrice(id);
}

export function resetModelBasePricesToBuiltins() {
  return getActiveBackendClient().economics.resetModelBasePricesToBuiltins();
}

export function getModelPriceSyncState() {
  return getActiveBackendClient().economics.getModelPriceSyncState();
}

export function saveModelPriceSyncConfig(input: ModelPriceSyncConfig) {
  return getActiveBackendClient().economics.saveModelPriceSyncConfig(input);
}

export function syncModelPrices(force = false) {
  return getActiveBackendClient().economics.syncModelPrices(force);
}

export function reloadModelPriceCatalog() {
  return getActiveBackendClient().economics.reloadModelPriceCatalog();
}

export function openModelPriceCatalogDirectory() {
  return getActiveBackendClient().economics.openModelPriceCatalogDirectory();
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
