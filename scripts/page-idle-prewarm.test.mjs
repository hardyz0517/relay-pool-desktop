import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const hook = await readFile("src/app/useIdlePagePrewarm.ts", "utf8").catch(() => "");
const policy = await readFile("src/app/pageTransitionPolicy.ts", "utf8");

assert.match(hook, /requestIdleCallback/);
assert.match(hook, /pointerdown/);
assert.match(hook, /keydown/);
assert.match(hook, /isInputPending/);
assert.match(policy, /prewarmPriority/);
assert.match(policy, /stations:[\s\S]*prewarmPriority:\s*1/);
assert.match(policy, /settings:[\s\S]*prewarmPriority:\s*2/);
assert.match(policy, /changes:[\s\S]*prewarmPriority:\s*3/);

console.log("page idle prewarm contract passed");
