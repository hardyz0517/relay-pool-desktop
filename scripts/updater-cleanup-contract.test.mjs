import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const commands = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLib = await readFile("src-tauri/src/lib.rs", "utf8");
const updaterApi = await readFile("src/lib/api/updater.ts", "utf8").catch(() => "");
const proxyApi = await readFile("src/lib/api/proxy.ts", "utf8").catch(() => "");
const provider = await readFile("src/features/updater/UpdaterProvider.tsx", "utf8").catch(() => "");

assert.ok(commands.includes("pub fn prepare_local_proxy_for_update"));
assert.ok(commands.includes("proxy.prepare_for_update"));
assert.ok(tauriLib.includes("commands::prepare_local_proxy_for_update"));
assert.ok(proxyApi.includes('invoke<ProxyStatus>("prepare_local_proxy_for_update")'));
assert.ok(provider.includes("prepareLocalProxyForUpdate"));
assert.ok(!updaterApi.includes("cleanup_before_update"));
assert.ok(!provider.includes("cleanupBeforeUpdate"));

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
  assert.ok(
    !source.includes('invoke("prepare_local_proxy_for_update"') &&
      !source.includes('invoke<ProxyStatus>("prepare_local_proxy_for_update"'),
    `${file} bypasses the shared proxy API`,
  );
}

console.log("updater drain-aware preparation boundary checks passed");
