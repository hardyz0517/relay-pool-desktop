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
  pageSource.includes("requestLogsQueryOptions") &&
    pageSource.includes("keyPoolQueryOptions") &&
    pageSource.includes("settingsQueryOptions") &&
    !pageSource.includes("loadRequestLogWorkspace"),
  "logs page should consume shared resource query options instead of the legacy workspace loader",
);

assert.ok(
  !/Promise\.all\(\[\s*listRequestLogs\(\),\s*listKeyPoolItems\(\),?\s*\]\)/s.test(pageSource),
  "logs page should no longer own raw fact Promise.all orchestration",
);
