import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const querySource = await readFile("src/lib/queries/logQueries.ts", "utf8");
const pageSource = await readFile("src/features/logs/LogsPage.tsx", "utf8");

assert.ok(
  querySource.includes("export type RequestLogWorkspace") &&
    querySource.includes("requestLogs: RequestLog[]") &&
    querySource.includes("keyPoolItems: KeyPoolItem[]"),
  "log query service should expose a raw facts workspace shape",
);

assert.ok(
  querySource.includes("export async function loadRequestLogWorkspace()") &&
    querySource.includes("listRequestLogs()") &&
    querySource.includes("listKeyPoolItems()"),
  "log query service should orchestrate existing raw fact reads",
);

assert.ok(
  !querySource.includes("formatTime") &&
    !querySource.includes("formatKeyName") &&
    !querySource.includes("formatCost") &&
    !querySource.includes("parseRejectedCandidates") &&
    !querySource.includes("clearRequestLogs"),
  "log query service must not define log page view behavior or write actions",
);

assert.ok(
  pageSource.includes('import { loadRequestLogWorkspace } from "@/lib/queries/logQueries";') &&
    pageSource.includes("const workspace = await loadRequestLogWorkspace()") &&
    pageSource.includes("setLogs(workspace.requestLogs)") &&
    pageSource.includes("setKeys(workspace.keyPoolItems)") &&
    pageSource.includes("workspace.requestLogs[0]?.id"),
  "logs page should consume the query service without changing existing state assignments",
);

assert.ok(
  !/Promise\.all\(\[\s*listRequestLogs\(\),\s*listKeyPoolItems\(\),?\s*\]\)/s.test(pageSource),
  "logs page should no longer own raw fact Promise.all orchestration",
);
