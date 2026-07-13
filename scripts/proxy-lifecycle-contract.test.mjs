import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const proxyApi = await readFile(new URL("../src/lib/api/proxy.ts", import.meta.url), "utf8");

const runningTransitions = proxyApi.match(
  /running: true,\s*\r?\n\s*lifecycle: "running"/g,
);
assert.equal(
  runningTransitions?.length ?? 0,
  2,
  "start and restart browser fallbacks must report the running lifecycle",
);
assert.match(
  proxyApi,
  /running: false,\s*\r?\n\s*lifecycle: "stopped",\s*\r?\n\s*activeRequests: 0/,
  "stop browser fallback must report the stopped lifecycle",
);

console.log("proxy lifecycle fallback contract passed");
