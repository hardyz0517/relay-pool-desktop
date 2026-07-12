import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const activity = await readFile("src/components/shell/PageActivity.tsx", "utf8");
const query = await readFile("src/lib/query/useActivityQuery.ts", "utf8").catch(() => "");

assert.match(activity, /type PageActivity = \{/);
assert.match(activity, /interactive: boolean/);
assert.match(activity, /refreshEnabled: boolean/);
assert.match(activity, /export function usePageActivity/);
assert.match(query, /enabled:\s*queryEnabled/);
assert.match(query, /subscribed:\s*active/);
assert.match(query, /recordHiddenPageQueryStart/);
assert.ok(!query.includes("setInterval"));

console.log("page activity query contract passed");
