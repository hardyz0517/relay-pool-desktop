import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { ToastProvider } from "@/components/ui";
import { installDesktopWebViewGuards } from "@/lib/desktopGuards";
import { UpdaterProvider } from "@/features/updater/UpdaterProvider";
import { QueryErrorNotifier } from "@/lib/query/QueryErrorNotifier";
import { queryClient } from "@/lib/query/queryClient";
import { ThemeProvider } from "@/theme/ThemeProvider";
import { initializeTheme } from "@/theme/themeBootstrap";
import { App } from "@/app/App";
import { BackendBootstrap } from "@/app/bootstrap/BackendBootstrap";
import { DataStoreBootstrap } from "@/features/data-recovery/DataStoreBootstrap";
import { createDesktopBackendClient } from "@/lib/bridge/DesktopBackend";
import "@/styles.css";

const initialTheme = initializeTheme();

installDesktopWebViewGuards();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider initialSnapshot={initialTheme}>
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <QueryErrorNotifier />
          <BackendBootstrap
            createClient={createDesktopBackendClient}
            renderDataStoreBootstrap={(renderReady) => <DataStoreBootstrap renderReady={renderReady} />}
            renderReady={() => (
              <UpdaterProvider>
                <App />
              </UpdaterProvider>
            )}
          />
        </ToastProvider>
      </QueryClientProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
