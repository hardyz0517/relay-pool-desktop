import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const mainSource = await readFile("src/main.tsx", "utf8");
const backendBootstrapSource = await readFile("src/app/bootstrap/BackendBootstrap.tsx", "utf8");
const dataStoreBootstrapSource = await readFile("src/features/data-recovery/DataStoreBootstrap.tsx", "utf8");
const dataRecoveryScreenSource = await readFile("src/features/data-recovery/DataRecoveryScreen.tsx", "utf8");
const updaterProviderSource = await readFile("src/features/updater/UpdaterProvider.tsx", "utf8");
const updateDialogSource = await readFile("src/features/updater/UpdateDialog.tsx", "utf8");

assert.ok(
  mainSource.includes("<QueryClientProvider client={queryClient}>") &&
    mainSource.includes("renderDataStoreBootstrap={(renderReady) => <DataStoreBootstrap renderReady={renderReady} />}") &&
    mainSource.indexOf("renderDataStoreBootstrap") < mainSource.indexOf("<UpdaterProvider>") &&
    mainSource.indexOf("<UpdaterProvider>") < mainSource.indexOf("<App />"),
  "desktop startup should keep DataStoreBootstrap before UpdaterProvider and the business App",
);

assert.ok(
  backendBootstrapSource.includes('client.mode === "demo"') &&
    backendBootstrapSource.includes('setState({ kind: "DemoReady", client, contract })') &&
    backendBootstrapSource.includes('setState({ kind: "DataStoreBootstrapping", client, contract })') &&
    backendBootstrapSource.includes("renderDataStoreBootstrap ? renderDataStoreBootstrap(readyNode) : readyNode()"),
  "BackendBootstrap should route demo directly to ready and desktop through the datastore bootstrap gate",
);

assert.ok(
  dataStoreBootstrapSource.includes("getDataStoreStartupState()") &&
    dataStoreBootstrapSource.includes("requestSequence.current") &&
    dataStoreBootstrapSource.includes('state.decision.kind !== "ready"') &&
    dataStoreBootstrapSource.includes("<DataRecoveryScreen state={status.state} onActivated={reload} />") &&
    !dataStoreBootstrapSource.includes("useActivityQuery") &&
    !dataStoreBootstrapSource.includes("useQuery") &&
    !dataStoreBootstrapSource.includes("queryKeys") &&
    !dataStoreBootstrapSource.includes("AppShell"),
  "DataStoreBootstrap should remain a fail-closed startup gate, not a page activity query owner",
);

assert.ok(
  dataRecoveryScreenSource.includes('activeOperation, setActiveOperation') &&
    dataRecoveryScreenSource.includes('setActiveOperation("activate")') &&
    dataRecoveryScreenSource.includes('setActiveOperation("locate")') &&
    dataRecoveryScreenSource.includes('setActiveOperation("create")') &&
    dataRecoveryScreenSource.includes('setActiveOperation("diagnostic")') &&
    dataRecoveryScreenSource.includes('setActiveOperation("backup")') &&
    dataRecoveryScreenSource.includes("restartApp()") &&
    dataRecoveryScreenSource.includes("onActivated()") &&
    !dataRecoveryScreenSource.includes("useActivityQuery") &&
    !dataRecoveryScreenSource.includes("queryKeys"),
  "DataRecoveryScreen should keep recovery commands as explicit foreground operations, not cached server-state reads",
);

assert.ok(
  updaterProviderSource.includes("useReducer(reduceUpdaterState, initialUpdaterState)") &&
    updaterProviderSource.includes("checkingRef") &&
    updaterProviderSource.includes("installingRef") &&
    updaterProviderSource.includes("downloadPendingUpdate((progress)") &&
    updaterProviderSource.includes("prepareLocalProxyForUpdate()") &&
    updaterProviderSource.includes("queryClient.setQueryData(queryKeys.proxyStatus, nextProxyStatus)") &&
    updaterProviderSource.includes("installPendingUpdateAndRelaunch()") &&
    !updaterProviderSource.includes("useActivityQuery") &&
    !updaterProviderSource.includes("setInterval"),
  "UpdaterProvider should stay a foreground operation controller with bounded native updater ownership",
);

assert.ok(
  updateDialogSource.includes('state.phase === "checking"') &&
    updateDialogSource.includes('state.phase === "downloading"') &&
    updateDialogSource.includes('state.phase === "cleaning"') &&
    updateDialogSource.includes('state.phase === "installing"') &&
    updateDialogSource.includes("downloadedBytes") &&
    updateDialogSource.includes("totalBytes") &&
    !updateDialogSource.includes("useQuery") &&
    !updateDialogSource.includes("queryKeys"),
  "UpdateDialog should remain a pure projection of the updater operation state",
);

console.log("data recovery and updater owner contract passed");
