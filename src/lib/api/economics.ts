import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  RequestKind,
  ModelPriceSyncConfig,
  UpsertModelBasePriceInput,
} from "@/lib/types/economics";

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

export function listCurrentStationBalanceSnapshots() {
  return getActiveBackendClient().economics.listCurrentStationBalanceSnapshots();
}

export function listBalanceSnapshotsForStation(stationId: string) {
  return getActiveBackendClient().economics.listBalanceSnapshotsForStation(stationId);
}
