import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";

const sourceRoot = "src";
const sourceExtensions = new Set([".ts", ".tsx"]);
const forbiddenCopy = "降级";

async function collectSourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...await collectSourceFiles(path));
    } else if (sourceExtensions.has(path.slice(path.lastIndexOf(".")))) {
      files.push(path);
    }
  }
  return files;
}

const violations = [];
for (const path of await collectSourceFiles(sourceRoot)) {
  const source = await readFile(path, "utf8");
  if (!source.includes(forbiddenCopy)) continue;
  const lineNumbers = source
    .split(/\r?\n/u)
    .map((line, index) => line.includes(forbiddenCopy) ? index + 1 : null)
    .filter((line) => line !== null);
  violations.push(`${relative(process.cwd(), path)}:${lineNumbers.join(",")}`);
}

assert.deepEqual(
  violations,
  [],
  "user-facing source must use 欠佳 instead of the retired 降级 copy",
);

console.log("UI terminology gate passed");
