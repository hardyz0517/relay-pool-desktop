import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("vite.config.ts", "utf8");

for (const pattern of ["**/.worktrees/**", "**/dist/**", "**/.pnpm-store/**"]) {
  assert.ok(
    source.includes(`"${pattern}"`),
    `vite dev server should ignore ${pattern} so unrelated generated files do not restart the desktop app`,
  );
}

assert.ok(
  source.includes('"**/src-tauri/target/**"'),
  "vite dev server should keep ignoring Rust target output",
);

console.log("vite watch ignore runtime noise contract passed");
