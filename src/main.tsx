import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "@/app/App";
import { ToastProvider } from "@/components/ui";
import { installDesktopWebViewGuards } from "@/lib/desktopGuards";
import { UpdaterProvider } from "@/features/updater/UpdaterProvider";
import "@/styles.css";

installDesktopWebViewGuards();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ToastProvider>
      <UpdaterProvider>
        <App />
      </UpdaterProvider>
    </ToastProvider>
  </React.StrictMode>,
);
