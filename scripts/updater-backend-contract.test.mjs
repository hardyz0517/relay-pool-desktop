import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const commands = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const lib = await readFile("src-tauri/src/lib.rs", "utf8");
const proxyApi = await readFile("src/lib/api/proxy.ts", "utf8");

assert.ok(
  commands.includes("pub fn prepare_local_proxy_for_update"),
  "commands should expose prepare_local_proxy_for_update",
);
assert.ok(
  commands.includes("Duration::from_secs(30)"),
  "update preparation should use a 30 second drain timeout",
);
assert.ok(
  lib.includes("commands::prepare_local_proxy_for_update"),
  "Tauri invoke handler should register prepare_local_proxy_for_update",
);
assert.ok(
  proxyApi.includes('invoke<ProxyStatus>("prepare_local_proxy_for_update")'),
  "proxy API should expose the update preparation command",
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
