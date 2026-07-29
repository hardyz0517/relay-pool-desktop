import type { PortableMigrationOperation } from "@/lib/types/dataMigration";
import { operationProgressLabel, terminalLabel } from "@/lib/dataMigrationViewModel";

type ImportMigrationSummaryProps = {
  operation: PortableMigrationOperation | null;
};

export function ImportMigrationSummary({ operation }: ImportMigrationSummaryProps) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-muted/40 px-3 py-2 text-xs text-muted-foreground">
      <div className="font-medium text-foreground">当前迁移操作</div>
      <div className="mt-1">{operationProgressLabel(operation)}</div>
      {terminalLabel(operation) ? <div className="mt-1">{terminalLabel(operation)}</div> : null}
    </div>
  );
}
