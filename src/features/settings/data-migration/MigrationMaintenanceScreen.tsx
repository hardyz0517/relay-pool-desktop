import { Button, Card } from "@/components/ui";
import type { PortableImportRecoveryState } from "@/lib/types/dataMigration";
import { describeRecoveryState } from "./migrationViewModel";

type MigrationMaintenanceScreenProps = {
  state: PortableImportRecoveryState;
  onRestart: () => void;
  onRetry: () => void;
};

export function MigrationMaintenanceScreen({
  state,
  onRestart,
  onRetry,
}: MigrationMaintenanceScreenProps) {
  const view = describeRecoveryState(state);
  return (
    <main className="flex min-h-screen items-center justify-center bg-app px-6 text-foreground">
      <Card className="w-full max-w-[640px] p-6">
        <p className="text-sm font-semibold">{view.title}</p>
        <p className="mt-2 text-sm text-muted-foreground">{view.detail}</p>
        <div className="mt-5 flex flex-wrap gap-2">
          {state.state === "activationPending" ? (
            <Button onClick={onRestart}>重启完成激活</Button>
          ) : null}
          <Button variant="secondary" onClick={onRetry}>重新检查</Button>
        </div>
      </Card>
    </main>
  );
}
