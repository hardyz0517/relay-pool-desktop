import assert from "node:assert/strict";
import { existsSync } from "node:fs";

assert.equal(
  existsSync("src/lib/projections/runtimeSnapshot.ts"),
  false,
  "legacy frontend runtime snapshot projection must stay deleted; routing runtime/read-model facts now come from backend projections",
);
