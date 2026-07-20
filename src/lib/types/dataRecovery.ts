export type DatabaseGeneration = "one" | "two";

export type DataStoreRuntimeMode = "writable" | "inspectionOnly" | "recovery";

export type CompatibilityDecisionCode =
  | "writable"
  | "inspectionOnly"
  | "generationMismatch"
  | "readerTooOld"
  | "writerTooOld"
  | "metadataMismatch";

export type SchemaCompatibilityView = {
  decisionCode: CompatibilityDecisionCode;
  schemaVersion: number | null;
  appVersion: string;
};

export type DataRecoveryCapabilities = {
  canBackup: boolean;
  canExportDiagnostic: boolean;
  canCheckForUpdates: boolean;
  canLocateCandidate: boolean;
  canActivateCandidate: boolean;
  canCreateDataStore: boolean;
};

export type RecoveryReason =
  | "missing"
  | "unreadable"
  | "invalidSqlite"
  | "integrityFailed"
  | "openOrMigrationFailed"
  | "pendingRelocation"
  | "unsupportedLegacySchema"
  | "incompatibleSchema"
  | "upgradeRecoveryRequired"
  | "relocationUpgradeConflict"
  | "generationReopenFailed";

export type DataStoreStartupDecision =
  | { kind: "ready"; candidateId: string }
  | { kind: "inspectionOnly"; candidateId: string; reason: CompatibilityDecisionCode }
  | { kind: "firstRun"; defaultDataDir: string }
  | { kind: "needsRecovery"; reason: RecoveryReason }
  | { kind: "conflict"; candidateIds: string[] };

export type DataStoreCandidate = {
  id: string;
  role: "active" | "default" | "source" | "pending" | "backup" | "located";
  path: string;
  health: "healthy" | "missing" | "unreadable" | "invalidSqlite" | "integrityFailed";
  databaseGeneration: DatabaseGeneration | null;
  compatibility: SchemaCompatibilityView | null;
  sizeBytes: number | null;
  modifiedAt: string | null;
  counts: Record<string, number>;
};

type DataStoreStartupViewBase = {
  databaseGeneration: DatabaseGeneration;
  compatibility: SchemaCompatibilityView | null;
  capabilities: DataRecoveryCapabilities;
  candidates: DataStoreCandidate[];
};

export type DataStoreStartupView = DataStoreStartupViewBase & (
  | {
    mode: "writable";
    decision: Extract<DataStoreStartupDecision, { kind: "ready" }>;
    compatibility: SchemaCompatibilityView & { decisionCode: "writable" };
  }
  | {
    mode: "inspectionOnly";
    decision: Extract<DataStoreStartupDecision, { kind: "inspectionOnly" }>;
    compatibility: SchemaCompatibilityView;
  }
  | {
    mode: "recovery";
    decision: Exclude<DataStoreStartupDecision, { kind: "ready" | "inspectionOnly" }>;
  }
);

export type ActivationResult = {
  restartRequired: boolean;
};
