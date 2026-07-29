import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const localProxyCommands = await readFile("src-tauri/src/commands/local_proxy.rs", "utf8");
const registry = await readFile("src-tauri/src/ipc/registry.rs", "utf8");
const updaterApi = await readFile("src/lib/api/updater.ts", "utf8").catch(() => "");
const proxyApi = await readFile("src/lib/api/proxy.ts", "utf8").catch(() => "");
const desktopBackend = await readFile("src/lib/bridge/DesktopBackend.ts", "utf8").catch(() => "");
const provider = await readFile("src/lib/updater/UpdaterProvider.tsx", "utf8").catch(() => "");

assert.ok(localProxyCommands.includes("pub async fn prepare_local_proxy_for_update"));
assert.ok(localProxyCommands.includes("proxy.prepare_for_update"));
assert.ok(registry.includes("prepare_local_proxy_for_update => $crate::commands::local_proxy::prepare_local_proxy_for_update"));
assert.ok(proxyApi.includes("getActiveBackendClient().proxy.prepareLocalProxyForUpdate()"));
assert.ok(desktopBackend.includes("prepareLocalProxyForUpdate: () => prepareLocalProxyForUpdateBinding()"));
assert.ok(!proxyApi.includes('invoke<ProxyStatus>("prepare_local_proxy_for_update")'));
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
