export type RecoveryReason =
  | "missing"
  | "unreadable"
  | "invalidSqlite"
  | "integrityFailed"
  | "openOrMigrationFailed"
  | "pendingRelocation";

export type DataStoreStartupDecision =
  | { kind: "ready"; candidateId: string }
  | { kind: "firstRun"; defaultDataDir: string }
  | { kind: "needsRecovery"; reason: RecoveryReason }
  | { kind: "conflict"; candidateIds: string[] };

export type DataStoreCandidate = {
  id: string;
  role: "active" | "default" | "source" | "pending" | "backup" | "located";
  path: string;
  health: "healthy" | "missing" | "unreadable" | "invalidSqlite" | "integrityFailed";
  schemaCompatible: boolean;
  sizeBytes: number | null;
  modifiedAt: string | null;
  counts: Record<string, number>;
};

export type DataStoreStartupView = {
  decision: DataStoreStartupDecision;
  candidates: DataStoreCandidate[];
};

export type ActivationResult = {
  restartRequired: boolean;
};
