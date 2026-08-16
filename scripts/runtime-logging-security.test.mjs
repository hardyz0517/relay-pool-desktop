import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
// Keep this list aligned with docs/audits/runtime-logging-canary-matrix.md.
// These are deliberately fake values: the gate only proves that production
// source cannot accidentally bake sensitive-shaped payloads into logging
// paths. Producer/bundle tests separately assert redaction at runtime.
const forbidden = [
  "sk-secret",
  "Authorization: Bearer",
  "cookie=fake-cookie",
  "fake-password",
  "https://user:pass@example.test/v1/x?token=fake#frag",
  "C:\\\\Users\\\\fixture\\\\relay-pool.db",
  "prompt fixture",
  "response fixture",
];
const sourceRoots = [path.join(root, "src-tauri", "src"), path.join(root, "src")];
const violations = [];

function walk(directory) {
  if (!fs.existsSync(directory)) return;
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (entry.isFile() && /\.(rs|ts|tsx|mjs)$/.test(entry.name)) {
      if (/\.test\.(rs|ts|tsx|mjs)$/.test(entry.name) || entry.name.endsWith("_test.rs")) continue;
      const text = fs.readFileSync(full, "utf8");
      const productionText = text.split(/\n\s*#\[cfg\(test\)\]/, 1)[0];
      for (const marker of forbidden) {
        if (productionText.includes(marker) && !full.includes(`${path.sep}test`)) {
          violations.push(`${path.relative(root, full)} contains ${marker}`);
        }
      }
    }
  }
}

for (const sourceRoot of sourceRoots) walk(sourceRoot);
if (violations.length) {
  console.error("runtime logging security canary violations:");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exit(1);
}
console.log("runtime logging security source scan passed");
