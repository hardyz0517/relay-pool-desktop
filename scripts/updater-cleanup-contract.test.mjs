import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const commands = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLib = await readFile("src-tauri/src/lib.rs", "utf8");
const updaterApi = await readFile("src/lib/api/updater.ts", "utf8").catch(() => "");

assert.ok(commands.includes("pub fn cleanup_before_update"));
assert.ok(commands.includes("proxy.cleanup_before_update"));
assert.ok(tauriLib.includes("commands::cleanup_before_update"));
assert.ok(updaterApi.includes('invoke<ProxyStatus>("cleanup_before_update")'));

const featureFiles = [];
async function collect(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const next = path.join(dir, entry.name);
    if (entry.isDirectory()) await collect(next);
    else if (/\.(ts|tsx)$/.test(entry.name)) featureFiles.push(next);
  }
}
await collect("src/features");
for (const file of featureFiles) {
  const source = await readFile(file, "utf8");
  assert.ok(!source.includes('invoke("cleanup_before_update"'), `${file} bypasses updater API`);
}

console.log("updater cleanup boundary checks passed");
