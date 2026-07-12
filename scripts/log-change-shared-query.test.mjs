import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const logs = await readFile("src/features/logs/LogsPage.tsx", "utf8");
const changes = await readFile("src/features/changes/ChangeCenterPage.tsx", "utf8");

assert.match(logs, /requestLogsQueryOptions/);
assert.match(logs, /keyPoolQueryOptions/);
assert.match(logs, /settingsQueryOptions/);
assert.match(logs, /proxyStatusQueryOptions/);
assert.ok(!logs.includes("loadRequestLogWorkspace"));
assert.ok(!logs.includes("setLogs("));

assert.match(changes, /changeEventsQueryOptions/);
assert.match(changes, /stationsQueryOptions/);
assert.match(changes, /queryClient\.setQueryData\(queryKeys\.changeEvents/);
assert.ok(!changes.includes("loadChangeCenterWorkspace"));
assert.ok(!changes.includes("setEvents("));

console.log("log and change shared query contract passed");
