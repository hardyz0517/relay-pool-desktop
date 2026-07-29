export type PortableMigrationBlockedReason =
  | "security_policy_not_approved"
  | "unsupported_platform"
  | "security_baseline_incomplete"
  | "credential_store_key_missing"
  | "credential_store_unavailable"
  | "data_store_not_writable"
  | "maintenance_in_progress";

export type PortableMigrationLimits = {
  maxAgeFileBytes: number;
  maxSqliteBytes: number;
  maxRowsPerTable: number;
  maxTotalUserTableRows: number;
  maxJsonDepth: number;
  maxRegularFieldBytes: number;
  maxLargeRedactedJsonFieldBytes: number;
  maxPassphraseUtf8Bytes: number;
  exportDeadlineMs: number;
  inspectionDeadlineMs: number;
  prepareDeadlineMs: number;
};

export type PortableMigrationCapability = {
  enabled: boolean;
  blockedReasons: PortableMigrationBlockedReason[];
  supportedFormat: string;
  supportedProfile: string;
  currentSchemaProfile: string;
  historySupported: boolean;
  limits: PortableMigrationLimits;
};

export type PortablePathToken = {
  pathToken: string;
  expiresInMs: number;
};

export type StartPortableExportInput = {
  outputPathToken: string;
  passphrase: string;
  passphraseConfirmation: string;
  options: { includeHistory: boolean };
  idempotencyKey: string;
};

export type InspectPortableImportInput = {
  inputPathToken: string;
  passphrase: string;
  idempotencyKey: string;
};

export type PortableImportMode = "restoreIntoEmpty" | "replaceCurrent";

export type PreparePortableImportInput = {
  inspectedImportId: string;
  mode: PortableImportMode;
  confirmationText: string;
  idempotencyKey: string;
};

export type PortableMigrationResourceKind = "export" | "inspection" | "import";

export type PortableMigrationOperationStarted = {
  operationId: string;
  resourceId: string;
  resourceKind: PortableMigrationResourceKind;
};

export type PortableMigrationOperationKind = "export_package" | "inspect_package" | "prepare_import";
export type PortableMigrationOperationState = "running" | "stopping" | "terminal";

export type PortableMigrationProgress =
  | { phase: "queued" }
  | { phase: "kdf_started" }
  | { phase: "kdf_finished" }
  | { phase: "reading_package"; percent: number; bytesRead: number }
  | { phase: "writing_database"; percent: number; rowsWritten: number }
  | { phase: "publishing_package"; percent: number; bytesWritten: number }
  | { phase: "verifying_package" };

export type PortableMigrationTerminalResult =
  | { result: "exported_package"; exportId: string; packageSizeBytes: number }
  | {
      result: "inspected_package";
      exportId: string;
      sourcePlatform: string;
      includedCategories: string[];
      sqliteSizeBytes: number;
    }
  | { result: "prepared_import"; exportId: string; targetRows: number };

export type PortableMigrationTerminal =
  | { terminal: "completed"; result: PortableMigrationTerminalResult }
  | { terminal: "failed"; code: string }
  | { terminal: "cancelled" }
  | { terminal: "timed_out" }
  | { terminal: "result_unknown" };

export type PortableMigrationOperation = {
  operationId: string;
  kind: PortableMigrationOperationKind;
  state: PortableMigrationOperationState;
  deadlineMs: number;
  progress: PortableMigrationProgress[];
  terminal: PortableMigrationTerminal | null;
};

export type PortableExportResult = {
  exportId: string;
  packageSizeBytes: number;
};

export type PortableImportInspection = {
  inspectionId: string;
  exportId: string;
  sourcePlatform: string;
  includedCategories: string[];
  includeHistory: boolean;
  sqliteSizeBytes: number;
};

export type PortableImportPrepareResult = {
  importId: string;
  restartRequired: boolean;
};

export type PortableImportRecoveryReasonCode =
  | "activation_validation_failed"
  | "atomic_replace_failed"
  | "journal_invalid"
  | "artifact_identity_mismatch"
  | "rollback_validation_failed";

export type PortableImportRecoveryState =
  | { state: "none" }
  | { state: "activationPending"; importId: string }
  | { state: "activated"; importId: string }
  | { state: "rolledBack"; importId: string; reasonCode: PortableImportRecoveryReasonCode }
  | { state: "manualRecoveryRequired"; importId: string | null; reasonCode: PortableImportRecoveryReasonCode };

export const REPLACE_CURRENT_CONFIRMATION = "替换当前数据";
