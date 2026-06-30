export type SecretMigrationReport = {
  migratedCount: number;
  skippedCount: number;
  failedCount: number;
  failures: string[];
};

export type SecretScanFinding = {
  tableName: string;
  columnName: string;
  evidence: string;
};
