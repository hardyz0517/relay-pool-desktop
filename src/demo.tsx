import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { BackendBootstrap } from "@/app/bootstrap/BackendBootstrap";
import { ToastProvider } from "@/components/ui";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/Card";
import { DemoBackend, createDemoBackendClient } from "@/lib/bridge/DemoBackend";
import { queryClient } from "@/lib/query/queryClient";
import { initializeTheme } from "@/theme/themeBootstrap";
import "@/styles.css";

initializeTheme();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <BackendBootstrap
          createClient={createDemoBackendClient}
          renderReady={({ client }) => (
            <DemoApp onReset={() => {
              if (client instanceof DemoBackend) client.reset();
            }} />
          )}
        />
      </ToastProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);

function DemoApp({ onReset }: { onReset: () => void }) {
  return (
    <main className="min-h-screen bg-app px-8 py-8 text-foreground" data-runtime-mode="demo">
      <div className="mx-auto flex w-full max-w-[960px] flex-col gap-4">
        <header>
          <p className="text-xs font-semibold uppercase tracking-[0.08em] text-muted-foreground">Demo runtime</p>
          <h1 className="mt-2 text-2xl font-semibold">Relay Pool Desktop</h1>
        </header>
        <Card className="p-5">
          <p className="text-sm font-semibold">Isolated preview backend is active</p>
          <p className="mt-2 text-sm text-muted-foreground">
            This entry uses deterministic demo state only. Desktop commands, data recovery, credentials, files, network calls, and updates are unavailable here.
          </p>
          <Button className="mt-4" variant="secondary" onClick={onReset}>Reset demo</Button>
        </Card>
      </div>
    </main>
  );
}
