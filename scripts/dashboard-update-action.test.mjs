import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const updaterProviderSource = await readFile("src/features/updater/UpdaterProvider.tsx", "utf8");

assert.ok(
  updaterProviderSource.includes("installNow: () => Promise<void>") &&
    updaterProviderSource.includes("showUpdateDialog: () => void") &&
    updaterProviderSource.includes("installNow: install") &&
    updaterProviderSource.includes("showUpdateDialog") &&
    updaterProviderSource.includes("if (shouldNotify) setDialogOpen(true);"),
  "updater provider should expose install and confirmation-dialog actions to page-level actions",
);

assert.ok(
  dashboardSource.includes('import { useUpdater } from "@/features/updater/UpdaterProvider";') &&
    dashboardSource.includes("const { state: updaterState, showUpdateDialog } = useUpdater();"),
  "dashboard should consume the shared updater state and confirmation-dialog action",
);

assert.ok(
  dashboardSource.includes("const updateAction = updaterState.phase === \"available\"") &&
    dashboardSource.includes("<IconButton") &&
    dashboardSource.includes("<ArrowUp") &&
    dashboardSource.includes("onClick={showUpdateDialog}"),
  "dashboard should render a top-right arrow that opens the update confirmation only when an update is available",
);

assert.ok(
  !dashboardSource.includes("onClick={() => void installNow()}"),
  "dashboard update arrow must not bypass the confirmation dialog by installing directly",
);

assert.ok(
  dashboardSource.includes("<PageScaffold title=\"总览\" actions={updateAction}>"),
  "dashboard should place the update action in the page header actions slot",
);

assert.doesNotMatch(
  dashboardSource,
  /updaterState\.phase !== "idle"[\s\S]{0,120}<IconButton/,
  "dashboard update action should not show for checking, failed, downloading, cleaning, or installing states",
);
