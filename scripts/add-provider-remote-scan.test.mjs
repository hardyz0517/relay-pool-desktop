import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pageSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
const controllerSource = await readFile("src/features/stations/useAddProviderPageController.ts", "utf8");
const sectionsSource = await readFile("src/features/stations/pages/add-provider/AddProviderSections.tsx", "utf8");
const remoteListSource = await readFile("src/features/stations/components/RemoteKeyDiscoveryList.tsx", "utf8");

assert.match(
  controllerSource,
  /await scanProviderDraftRemoteKeys\(\(await flushProviderDraft\(\)\)\.id\)/,
  "new provider remote scans should flush and scan the provider draft",
);

assert.match(
  controllerSource,
  /const createRemoteDisabled =[\s\S]*!activeStationId/,
  "remote creation must stay disabled until the draft is committed",
);

assert.ok(
  pageSource.includes("providerDraftId={providerDraftId}") &&
    sectionsSource.includes("(activeStationId || providerDraftId)"),
  "draft remote scan results should be visible before commit",
);

assert.ok(
  sectionsSource.includes("readOnly={!activeStationId}") &&
    remoteListSource.includes("loading || readOnly || deleteDisabled"),
  "draft remote results must expose no mutation controls",
);

console.log("provider draft remote scan source guard passed");
