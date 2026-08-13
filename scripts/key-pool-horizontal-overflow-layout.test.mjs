import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pageSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const rowsSource = await readFile("src/features/key-pool/KeyPoolRows.tsx", "utf8");

assert.match(
  pageSource,
  /<div className="overflow-x-auto">\s*<div className=\{keyPoolTableClassName\}>[\s\S]*?keyPoolGridClassName[\s\S]*?divide-y divide-border/,
  "key-pool header and rows should share one full scroll-width table canvas",
);
assert.match(rowsSource, /keyPoolTableClassName = "min-w-\[62rem\]"/);
assert.match(rowsSource, /grid-cols-\[[^\]]*10\.5rem\]/);
assert.match(rowsSource, /className=\{cn\("w-full will-change-transform"/);
