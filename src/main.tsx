import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { App } from "@/app/App";
import { ToastProvider } from "@/components/ui";
import { installDesktopWebViewGuards } from "@/lib/desktopGuards";
import { UpdaterProvider } from "@/features/updater/UpdaterProvider";
import { QueryErrorNotifier } from "@/lib/query/QueryErrorNotifier";
import { queryClient } from "@/lib/query/queryClient";
import "@/styles.css";

installDesktopWebViewGuards();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <ToastProvider>
        <QueryErrorNotifier />
        <UpdaterProvider>
          <App />
        </UpdaterProvider>
      </ToastProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
