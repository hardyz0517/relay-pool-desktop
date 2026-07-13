import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const routingPageSource = await readFile("src/features/routing/RoutingPage.tsx", "utf8");
const statusTabSource = await readFile("src/features/routing/LocalRoutingStatusTab.tsx", "utf8");
const statusRowSource = await readFile(
  "src/features/routing/LocalRoutingStatusCandidateRow.tsx",
  "utf8",
).catch(() => "");
const editRowSource = await readFile("src/features/routing/LocalRoutingCandidateRow.tsx", "utf8");
const clockSource = await readFile("src/features/routing/useCooldownClock.ts", "utf8").catch(
  () => "",
);

assert.match(routingPageSource, /useCooldownClock/);
assert.match(routingPageSource, /refreshEnabled\s*&&\s*activeTab\s*===\s*"status"/);
assert.match(statusTabSource, /nowMs=\{nowMs\}/);
assert.match(statusRowSource, /buildCooldownDisplay/);
assert.doesNotMatch(statusRowSource + editRowSource, /setInterval|setTimeout/);
assert.doesNotMatch(statusRowSource, /candidate\.cooldownUntil\s*\?\s*"/);
assert.match(clockSource, /window\.setInterval/);
assert.match(clockSource, /window\.clearInterval/);
assert.match(clockSource, /notifiedDeadlinesRef/);
