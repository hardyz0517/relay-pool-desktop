import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/Card";
import { getDataStoreStartupState, restartApp } from "@/lib/api/dataRecovery";
import { getPortableImportRecoveryState } from "@/lib/api/dataMigration";
import { describeRecoveryState } from "@/lib/dataMigrationViewModel";
import { errorMessage } from "@/lib/errorMessage";
import type { DataStoreStartupView } from "@/lib/types/dataRecovery";
import type { PortableImportRecoveryState } from "@/lib/types/dataMigration";
import { DataRecoveryScreen } from "./DataRecoveryScreen";
import { MigrationMaintenanceScreen } from "./MigrationMaintenanceScreen";

type BootstrapStatus =
  | { kind: "loading" }
  | { kind: "error"; error: unknown }
  | { kind: "migrationRecovery"; migrationRecovery: PortableImportRecoveryState }
  | { kind: "loaded"; state: DataStoreStartupView };

type DataStoreBootstrapProps = {
  renderReady: () => ReactNode;
};

export function DataStoreBootstrap({ renderReady }: DataStoreBootstrapProps) {
  const [status, setStatus] = useState<BootstrapStatus>({ kind: "loading" });
  const requestSequence = useRef(0);

  const reload = useCallback(() => {
    const requestId = ++requestSequence.current;
    setStatus({ kind: "loading" });
    void (async () => {
      const migrationRecovery = await getPortableImportRecoveryState();
      if (describeRecoveryState(migrationRecovery).blocksBusinessApp) {
        return { kind: "migrationRecovery" as const, migrationRecovery };
      }
      return { kind: "loaded" as const, state: await getDataStoreStartupState() };
    })().then(
      (nextStatus) => {
        if (requestSequence.current === requestId) setStatus(nextStatus);
      },
      (error) => {
        if (requestSequence.current === requestId) setStatus({ kind: "error", error });
      },
    );
  }, []);

  useEffect(() => {
    reload();
    return () => {
      requestSequence.current += 1;
    };
  }, [reload]);

  if (status.kind === "loading") return <StartupLoadingScreen />;
  if (status.kind === "error") return <StartupFatalError error={status.error} onRetry={reload} />;
  if (status.kind === "migrationRecovery") {
    return (
      <MigrationMaintenanceScreen
        state={status.migrationRecovery}
        onRestart={() => void restartApp()}
        onRetry={reload}
      />
    );
  }
  if (status.state.mode !== "writable" || status.state.decision.kind !== "ready") {
    return <DataRecoveryScreen state={status.state} onActivated={reload} />;
  }
  return <>{renderReady()}</>;
}

function StartupLoadingScreen() {
  return (
    <main className="flex min-h-screen items-center justify-center bg-app px-6 text-foreground">
      <Card className="w-full max-w-[520px] p-6">
        <p className="text-sm font-semibold">正在检查本地数据</p>
        <p className="mt-2 text-sm text-muted-foreground">
          Relay Pool 正在确认本次启动会打开正确的数据库，检查完成前不会加载价格、站点或采集页面。
        </p>
      </Card>
    </main>
  );
}

function StartupFatalError({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  return (
    <main className="flex min-h-screen items-center justify-center bg-app px-6 text-foreground">
      <Card className="w-full max-w-[640px] p-6">
        <p className="text-sm font-semibold text-danger-foreground">启动检查失败</p>
        <p className="mt-2 text-sm text-muted-foreground">
          Relay Pool 无法读取启动期数据状态。为避免误打开空数据库，业务页面已暂停加载。
        </p>
        <pre className="mt-4 max-h-40 overflow-auto rounded-[var(--surface-radius)] border border-border bg-muted px-3 py-2 text-xs text-muted-foreground">
          {errorMessage(error)}
        </pre>
        <Button className="mt-4" variant="secondary" onClick={onRetry}>重试</Button>
      </Card>
    </main>
  );
}
