import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { IncompatibleRuntimeScreen } from "@/app/IncompatibleRuntimeScreen";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/Card";
import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";
import type { RuntimeContractInfo } from "@/lib/bridge/contract";
import { unknownErrorMessage } from "@/lib/bridge/errorMessage";
import { isRuntimeContractMismatch } from "@/lib/bridge/runtimeContractError";

export type BackendClientFactory = () => BackendClient;

export type BackendReadyContext = {
  readonly client: BackendClient;
  readonly contract: RuntimeContractInfo;
};

type BackendBootstrapProps = {
  createClient: BackendClientFactory;
  renderReady: (context: BackendReadyContext) => ReactNode;
  renderDataStoreBootstrap?: (renderReady: () => ReactNode) => ReactNode;
};

type BootstrapState =
  | { kind: "SelectingMode" }
  | { kind: "HandshakingDesktop"; client: BackendClient }
  | { kind: "DataStoreBootstrapping"; client: BackendClient; contract: RuntimeContractInfo }
  | { kind: "Ready"; client: BackendClient; contract: RuntimeContractInfo }
  | { kind: "DemoReady"; client: BackendClient; contract: RuntimeContractInfo }
  | { kind: "RuntimeUnavailable"; client: BackendClient; error: unknown }
  | { kind: "IncompatibleRuntime"; client: BackendClient; error: unknown };

export function BackendBootstrap({
  createClient,
  renderDataStoreBootstrap,
  renderReady,
}: BackendBootstrapProps) {
  const fixedClient = useMemo(() => createClient(), [createClient]);
  const [state, setState] = useState<BootstrapState>({ kind: "SelectingMode" });
  const requestSequence = useRef(0);

  const start = useCallback(() => {
    const requestId = ++requestSequence.current;
    const client = fixedClient;
    setState(client.mode === "desktop" ? { kind: "HandshakingDesktop", client } : { kind: "SelectingMode" });
    void client.handshake().then(
      (contract) => {
        if (requestSequence.current !== requestId) return;
        setActiveBackendClient(client);
        if (client.mode === "demo") {
          setState({ kind: "DemoReady", client, contract });
        } else {
          setState({ kind: "DataStoreBootstrapping", client, contract });
        }
      },
      (error) => {
        if (requestSequence.current !== requestId) return;
        setActiveBackendClient(null);
        setState(isRuntimeContractMismatch(error)
          ? { kind: "IncompatibleRuntime", client, error }
          : { kind: "RuntimeUnavailable", client, error });
      },
    );
  }, [fixedClient]);

  useEffect(() => {
    start();
    return () => {
      requestSequence.current += 1;
      setActiveBackendClient(null);
    };
  }, [start]);

  if (state.kind === "SelectingMode" || state.kind === "HandshakingDesktop") {
    return <BackendLoadingScreen mode={fixedClient.mode} />;
  }
  if (state.kind === "RuntimeUnavailable") {
    return <RuntimeUnavailableScreen error={state.error} onRetry={start} />;
  }
  if (state.kind === "IncompatibleRuntime") {
    return <IncompatibleRuntimeScreen error={state.error} onRetry={start} />;
  }
  if (state.kind === "DemoReady") {
    return <>{renderReady({ client: state.client, contract: state.contract })}</>;
  }
  if (state.kind === "DataStoreBootstrapping") {
    const readyNode = () => (
      <ReadyMarker
        client={state.client}
        contract={state.contract}
        onReady={(client, contract) => setState({ kind: "Ready", client, contract })}
      >
        {renderReady({ client: state.client, contract: state.contract })}
      </ReadyMarker>
    );
    return <>{renderDataStoreBootstrap ? renderDataStoreBootstrap(readyNode) : readyNode()}</>;
  }
  return <>{renderReady({ client: state.client, contract: state.contract })}</>;
}

function ReadyMarker({
  children,
  client,
  contract,
  onReady,
}: {
  children: ReactNode;
  client: BackendClient;
  contract: RuntimeContractInfo;
  onReady: (client: BackendClient, contract: RuntimeContractInfo) => void;
}) {
  useEffect(() => onReady(client, contract), [client, contract, onReady]);
  return <>{children}</>;
}

function BackendLoadingScreen({ mode }: { mode: BackendClient["mode"] }) {
  return (
    <main className="flex min-h-screen items-center justify-center bg-app px-6 text-foreground">
      <Card className="w-full max-w-[520px] p-6">
        <p className="text-sm font-semibold">Starting Relay Pool</p>
        <p className="mt-2 text-sm text-muted-foreground">
          {mode === "desktop" ? "Checking the desktop runtime contract before loading local data." : "Preparing the isolated demo runtime."}
        </p>
      </Card>
    </main>
  );
}

function RuntimeUnavailableScreen({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  return (
    <main className="flex min-h-screen items-center justify-center bg-app px-6 text-foreground">
      <Card className="w-full max-w-[640px] p-6">
        <p className="text-sm font-semibold text-danger-foreground">Desktop runtime unavailable</p>
        <p className="mt-2 text-sm text-muted-foreground">
          Relay Pool could not reach the fixed desktop backend. Retry keeps the same desktop mode and will not switch to demo data.
        </p>
        <pre className="mt-4 max-h-40 overflow-auto rounded-[var(--surface-radius)] border border-border bg-muted px-3 py-2 text-xs text-muted-foreground">
          {unknownErrorMessage(error)}
        </pre>
        <Button className="mt-4" variant="secondary" onClick={onRetry}>Retry runtime</Button>
      </Card>
    </main>
  );
}
