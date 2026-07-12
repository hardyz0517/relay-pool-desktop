import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const candidateRowSource = await readFile(
  "src/features/routing/LocalRoutingCandidateRow.tsx",
  "utf8",
);

assert.match(
  candidateRowSource,
  /const isCoolingDown = candidate\.healthState === "cooldown";/,
  "candidate rows should derive active cooldown from healthState, not stale cooldownUntil timestamps",
);

assert.match(
  candidateRowSource,
  /\{\s*label:\s*"冷却",\s*value:\s*isCoolingDown \? "进行中" : "无",\s*tone:\s*isCoolingDown \? "warning" : "neutral"\s*\}/,
  "candidate cooldown metric should only show 进行中 while the backend says the health state is cooldown",
);

assert.doesNotMatch(
  candidateRowSource,
  /candidate\.cooldownUntil \? "进行中" : "无"/,
  "candidate rows should not show stale, expired cooldownUntil values as active cooldowns",
);
