import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pageSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
const controllerSource = await readFile("src/features/stations/useAddProviderPageController.ts", "utf8");
const sectionsSource = await readFile("src/features/stations/pages/add-provider/AddProviderSections.tsx", "utf8");
const captureCommands = await readFile("src-tauri/src/commands/capture.rs", "utf8");
const captureFacade = await readFile(
  "src-tauri/src/application/command_facades/capture.rs",
  "utf8",
);

assert.match(
  controllerSource,
  /if \(activeStationId\)[\s\S]*startManualAuthorization\(activeStationId\)[\s\S]*flushProviderDraft\(\)[\s\S]*startProviderDraftAuthorization\(draft\.id\)/,
  "manual authorization should support both saved stations and flushed drafts",
);

assert.ok(
  !controllerSource.includes("ensureStationForManualAuthorization"),
  "draft authorization should not use the temporary save-first guard",
);

assert.match(
  captureCommands,
  /pub async fn start_provider_draft_authorization[\s\S]*\.start_provider_draft_authorization\(app, input\.draft_id\)/,
  "the draft authorization command should delegate to the capture facade use case",
);

assert.match(
  captureFacade,
  /pub\(crate\) async fn start_provider_draft_authorization[\s\S]*prepare_provider_draft_capture_session_start\(draft_id\)/,
  "the capture facade should own draft authorization preparation",
);

assert.match(
  sectionsSource,
  /打开窗口授权[\s\S]*测试连通性/,
  "authorization button should appear to the left of connectivity test",
);

assert.match(pageSource, /onStartManualAuthorization=\{handleStartManualAuthorization\}/);

console.log("provider draft manual authorization source guard passed");
