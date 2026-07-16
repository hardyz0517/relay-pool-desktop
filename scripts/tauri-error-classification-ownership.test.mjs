import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

const apiDir = "src/lib/api";
const entries = await readdir(apiDir, { withFileTypes: true });
const apiFiles = entries
  .filter((entry) => entry.isFile() && entry.name.endsWith(".ts"))
  .map((entry) => join(apiDir, entry.name));

for (const file of apiFiles) {
  const source = await readFile(file, "utf8");
  assert.doesNotMatch(source, /function\s+isInvokeUnavailable\s*\(/, `${file} must use the shared Tauri error classifier`);

  if (/\bisTauriInvokeUnavailable\b/.test(source)) {
    assert.match(
      source,
      /from\s+["']@\/lib\/tauriErrors["']/,
      `${file} must import isTauriInvokeUnavailable from @/lib/tauriErrors`,
    );
  }
}
