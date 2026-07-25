import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { DataStoreCandidate, DataStoreStartupView } from "@/lib/types/dataRecovery";

export function getDataStoreStartupState(): Promise<DataStoreStartupView> {
  return getActiveBackendClient().dataRecovery.getDataStoreStartupState();
}

export function activateDataStoreCandidate(candidateId: string) {
  return getActiveBackendClient().dataRecovery.activateDataStoreCandidate(candidateId);
}

export function refreshDataStoreCandidates(): Promise<DataStoreStartupView> {
  return getActiveBackendClient().dataRecovery.refreshDataStoreCandidates();
}

export function locateDataStoreCandidate(): Promise<DataStoreCandidate | null> {
  return getActiveBackendClient().dataRecovery.locateDataStoreCandidate();
}

export function createNewDataStore(confirmed: boolean) {
  return getActiveBackendClient().dataRecovery.createNewDataStore(confirmed);
}

export function restartApp() {
  return getActiveBackendClient().dataRecovery.restartApp();
}

export function openDataStoreBackupDir() {
  return getActiveBackendClient().dataRecovery.openDataStoreBackupDir();
}

export function exportDataStoreDiagnostic() {
  return getActiveBackendClient().dataRecovery.exportDataStoreDiagnostic();
}
