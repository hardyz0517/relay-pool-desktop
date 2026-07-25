import {
  activateDataStoreCandidate as activateDataStoreCandidateBinding,
  createNewDataStore as createNewDataStoreBinding,
  exportDataStoreDiagnostic as exportDataStoreDiagnosticBinding,
  getDataStoreStartupState as getDataStoreStartupStateBinding,
  locateDataStoreCandidate as locateDataStoreCandidateBinding,
  openDataStoreBackupDir as openDataStoreBackupDirBinding,
  refreshDataStoreCandidates as refreshDataStoreCandidatesBinding,
} from "@/lib/bridge/generated";

import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type {
  DataStoreCandidate,
  DataStoreStartupView,
  SchemaCompatibilityView,
} from "@/lib/types/dataRecovery";

const browserPreviewStartupState: DataStoreStartupView = {
  mode: "writable",
  databaseGeneration: "two",
  compatibility: {
    decisionCode: "writable",
    schemaVersion: null,
    appVersion: "browser-preview",
  },
  capabilities: {
    canBackup: false,
    canExportDiagnostic: false,
    canCheckForUpdates: false,
    canLocateCandidate: false,
    canActivateCandidate: false,
    canCreateDataStore: false,
  },
  decision: { kind: "ready", candidateId: "browser-preview" },
  candidates: [],
};

export async function getDataStoreStartupState(): Promise<DataStoreStartupView> {
  try {
    return parseStartupView(await getDataStoreStartupStateBinding());
  } catch (error) {
    if (isTauriInvokeUnavailable(error)) {
      return browserPreviewStartupState;
    }
    throw error;
  }
}

export function activateDataStoreCandidate(candidateId: string) {
  return activateDataStoreCandidateBinding({ candidateId });
}

export function refreshDataStoreCandidates() {
  return refreshDataStoreCandidatesBinding().then(parseStartupView);
}

export function locateDataStoreCandidate() {
  return locateDataStoreCandidateBinding().then((value) => {
    if (value === null) return null;
    if (!isCandidate(value)) throw new Error("invalid data store candidate response");
    return value as DataStoreCandidate;
  });
}

export function createNewDataStore(confirmed: boolean) {
  return createNewDataStoreBinding({ confirmed });
}

export function openDataStoreBackupDir() {
  return openDataStoreBackupDirBinding();
}

export function exportDataStoreDiagnostic() {
  return exportDataStoreDiagnosticBinding();
}

function parseStartupView(value: unknown): DataStoreStartupView {
  if (!isRecord(value) || !isRuntimeMode(value.mode) || !isGeneration(value.databaseGeneration)) {
    throw new Error("invalid data store startup response");
  }
  if (!isCapabilities(value.capabilities) || !Array.isArray(value.candidates)) {
    throw new Error("invalid data store startup response");
  }
  if (!(value.compatibility === null || isCompatibility(value.compatibility))) {
    throw new Error("invalid data store startup response");
  }
  if (!value.candidates.every(isCandidate)) {
    throw new Error("invalid data store startup response");
  }
  if (!isDecisionForMode(value.decision, value.mode)) {
    throw new Error("invalid data store startup response");
  }
  if (value.mode === "writable"
    && (!isCompatibility(value.compatibility) || value.compatibility.decisionCode !== "writable")) {
    throw new Error("invalid data store startup response");
  }
  if (value.mode === "inspectionOnly" && !isCompatibility(value.compatibility)) {
    throw new Error("invalid data store startup response");
  }
  return value as DataStoreStartupView;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isRuntimeMode(value: unknown): value is DataStoreStartupView["mode"] {
  return value === "writable" || value === "inspectionOnly" || value === "recovery";
}

function isGeneration(value: unknown): value is DataStoreStartupView["databaseGeneration"] {
  return value === "one" || value === "two";
}

function isCapabilities(value: unknown): boolean {
  return isRecord(value)
    && [
      "canBackup",
      "canExportDiagnostic",
      "canCheckForUpdates",
      "canLocateCandidate",
      "canActivateCandidate",
      "canCreateDataStore",
    ].every((key) => typeof value[key] === "boolean");
}

function isCandidate(value: unknown): boolean {
  if (!isRecord(value)) return false;
  return typeof value.id === "string"
    && ["active", "default", "source", "pending", "backup", "located"].includes(String(value.role))
    && typeof value.path === "string"
    && ["healthy", "missing", "unreadable", "invalidSqlite", "integrityFailed"].includes(String(value.health))
    && (value.databaseGeneration === null || isGeneration(value.databaseGeneration))
    && (value.compatibility === null || isCompatibility(value.compatibility))
    && (typeof value.sizeBytes === "number" || value.sizeBytes === null)
    && (typeof value.modifiedAt === "string" || value.modifiedAt === null)
    && isRecord(value.counts)
    && Object.values(value.counts).every((count) => typeof count === "number");
}

function isCompatibility(value: unknown): value is SchemaCompatibilityView {
  if (!isRecord(value)) return false;
  return isCompatibilityDecision(value.decisionCode)
    && (value.schemaVersion === null || typeof value.schemaVersion === "number")
    && typeof value.appVersion === "string";
}

function isCompatibilityDecision(value: unknown): boolean {
  return [
    "writable",
    "inspectionOnly",
    "generationMismatch",
    "readerTooOld",
    "writerTooOld",
    "metadataMismatch",
  ].includes(String(value));
}

function isDecisionForMode(value: unknown, mode: DataStoreStartupView["mode"]): boolean {
  if (!isRecord(value) || typeof value.kind !== "string") return false;
  if (mode === "writable") {
    return value.kind === "ready" && typeof value.candidateId === "string";
  }
  if (mode === "inspectionOnly") {
    return value.kind === "inspectionOnly"
      && typeof value.candidateId === "string"
      && isCompatibilityDecision(value.reason);
  }
  if (value.kind === "firstRun") return typeof value.defaultDataDir === "string";
  if (value.kind === "conflict") {
    return Array.isArray(value.candidateIds)
      && value.candidateIds.every((candidateId) => typeof candidateId === "string");
  }
  return value.kind === "needsRecovery" && [
    "missing",
    "unreadable",
    "invalidSqlite",
    "integrityFailed",
    "openOrMigrationFailed",
    "pendingRelocation",
    "unsupportedLegacySchema",
    "incompatibleSchema",
    "upgradeRecoveryRequired",
    "relocationUpgradeConflict",
    "generationReopenFailed",
  ].includes(String(value.reason));
}
