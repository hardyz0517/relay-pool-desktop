import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

/* global console */

const pageSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const dialogSource = await readFile("src/features/key-pool/KeyConnectivityTestDialog.tsx", "utf8");
const controllerSource = await readFile("src/features/key-pool/useKeyPoolPageController.ts", "utf8");
const operationControllerSource = await readFile(
  "src/features/key-pool/connectivityOperationController.ts",
  "utf8",
);
const commandsSource = await readFile("src-tauri/src/commands/station_key_connectivity.rs", "utf8");
const facadeSource = await readFile(
  "src-tauri/src/application/command_facades/station_key_connectivity.rs",
  "utf8",
);

assert.ok(
  pageSource.includes("KeyConnectivityTestDialog") &&
    dialogSource.includes('data-testid="key-connectivity-test-dialog"') &&
    dialogSource.includes('data-testid="key-connectivity-console-spinner"'),
  "key pool page should render the dedicated connectivity test dialog",
);

assert.ok(
  controllerSource.includes("connectivityOperation.run(") &&
    controllerSource.includes("setDisplayedResponseText(result.message)") &&
    !controllerSource.includes('event.type === "delta"'),
  "the dialog should use operation lifecycle progress and the typed final result",
);

assert.ok(
  operationControllerSource.includes("getStationKeyConnectivityOperationResult({ operationId })") &&
    !operationControllerSource.includes("station_key_connectivity.result") &&
    !operationControllerSource.includes("JSON.parse"),
  "connectivity results must use the typed result command instead of progress-text JSON",
);

assert.ok(
  commandsSource.includes("pub async fn start_station_key_connectivity_operation") &&
    commandsSource.includes("pub async fn get_station_key_connectivity_operation_result") &&
    commandsSource.includes("facade.store_result(context.id, result.clone())") &&
    !commandsSource.includes("pub async fn test_station_key_connectivity") &&
    !commandsSource.includes("Channel<") &&
    !commandsSource.includes("_outbound_channel"),
  "Rust should expose one operation-based connectivity path with a typed result registry",
);

assert.ok(
  facadeSource.includes("CONNECTIVITY_RESULT_CAPACITY: usize = 64") &&
    facadeSource.includes("CONNECTIVITY_RESULT_TTL") &&
    facadeSource.includes("StationKeyConnectivityResultStore"),
  "typed connectivity results should have bounded capacity and retention",
);

await assert.rejects(
  access("src/lib/bridge/streamingAdapter.ts"),
  "the retired Channel streaming adapter must not return",
);

console.log("key pool connectivity operation contract passed");
