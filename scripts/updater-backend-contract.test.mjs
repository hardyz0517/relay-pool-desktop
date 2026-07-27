import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const localProxyCommands = await readFile("src-tauri/src/commands/local_proxy.rs", "utf8");
const registry = await readFile("src-tauri/src/ipc/registry.rs", "utf8");
const proxyApi = await readFile("src/lib/api/proxy.ts", "utf8");

assert.ok(
  localProxyCommands.includes("pub async fn prepare_local_proxy_for_update"),
  "local proxy commands should expose prepare_local_proxy_for_update",
);
assert.ok(
  localProxyCommands.includes("Duration::from_secs(30)"),
  "update preparation should use a 30 second drain timeout",
);
assert.ok(
  registry.includes("prepare_local_proxy_for_update => $crate::commands::local_proxy::prepare_local_proxy_for_update"),
  "Tauri command registry should register prepare_local_proxy_for_update",
);
assert.ok(
  proxyApi.includes("getActiveBackendClient().proxy.prepareLocalProxyForUpdate()") &&
    !proxyApi.includes('invoke<ProxyStatus>("prepare_local_proxy_for_update")'),
  "proxy API should expose the generated update preparation command",
);

for (const featureFile of await listSourceFiles("src/features")) {
  const source = await readFile(featureFile, "utf8");
  assert.ok(
    !source.includes('invoke<ProxyStatus>("prepare_local_proxy_for_update")') &&
      !source.includes('invoke("prepare_local_proxy_for_update")'),
    `${featureFile} should call the proxy API instead of invoking update preparation directly`,
  );
}

console.log("updater backend contract passed");

async function listSourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return listSourceFiles(entryPath);
      }
      return /\.[cm]?[jt]sx?$/.test(entry.name) ? [entryPath] : [];
    }),
  );
  return files.flat();
}
