import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const sourceFiles = [];
await collectSourceFiles(path.join(root, "src", "features"), sourceFiles);

const localReadErrorDefinitions = [];
for (const absolutePath of sourceFiles) {
  const source = await readFile(absolutePath, "utf8");
  if (/function\s+readError\s*\(/.test(source)) {
    localReadErrorDefinitions.push(normalizePath(path.relative(root, absolutePath)));
  }
}

assert.deepEqual(
  localReadErrorDefinitions,
  [],
  `feature pages should import readError from src/lib/errors.ts instead of redefining it:\n${localReadErrorDefinitions.join("\n")}`,
);

const sharedErrorsSource = await readFile(path.join(root, "src", "lib", "errors.ts"), "utf8");
assert.ok(
  sharedErrorsSource.includes("export function readError(error: unknown)"),
  "shared readError helper should remain exported from src/lib/errors.ts",
);

async function collectSourceFiles(directory, files) {
  const entries = await readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    const absolutePath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      await collectSourceFiles(absolutePath, files);
      continue;
    }
    if (/\.(ts|tsx)$/.test(entry.name)) {
      files.push(absolutePath);
    }
  }
}

function normalizePath(value) {
  return value.split(path.sep).join("/");
}
