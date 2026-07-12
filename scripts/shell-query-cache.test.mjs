import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const shell = await readFile("src/components/shell/AppShell.tsx", "utf8");
const resources = await readFile("src/lib/query/resourceQueries.ts", "utf8").catch(() => "");

assert.match(resources, /settingsQueryOptions/);
assert.match(resources, /proxyStatusQueryOptions/);
assert.match(resources, /changeEventsQueryOptions/);
assert.match(shell, /useQueryClient/);
assert.match(shell, /useQuery\(settingsQueryOptions\(\)\)/);
assert.match(shell, /useQuery\(proxyStatusQueryOptions\(2_000\)\)/);
assert.match(shell, /useQuery\(changeEventsQueryOptions\(10_000\)\)/);
assert.ok(!shell.includes("window.setInterval"));
assert.ok(!shell.includes("useState<ProxyStatus"));
assert.ok(!shell.includes("useState<ChangeEvent"));

console.log("shell query cache contract passed");
