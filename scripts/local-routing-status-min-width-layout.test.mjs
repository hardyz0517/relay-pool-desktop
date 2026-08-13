import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile(
  "src/features/routing/LocalRoutingStatusCandidateRow.tsx",
  "utf8",
);

assert.match(source, /text-muted-foreground md:grid/);
assert.match(source, /md:grid-cols-\[minmax\(220px,1\.6fr\)/);
assert.match(source, /md:items-center/);
assert.match(source, /md:hidden/);
assert.doesNotMatch(source, /lg:grid|lg:grid-cols|lg:items-center|lg:text-center|lg:hidden/);
