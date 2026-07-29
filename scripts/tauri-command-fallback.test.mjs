import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const esbuild = require("../node_modules/.pnpm/node_modules/esbuild");

const outFile = resolve(tmpdir(), "relay-pool-tauri-command-fallback.test.mjs");
await mkdir(dirname(outFile), { recursive: true });
await esbuild.build({
  entryPoints: ["src/lib/tauriErrors.ts"],
  outfile: outFile,
  bundle: true,
  platform: "node",
  format: "esm",
});

const { isTauriInvokeUnavailable } = await import(
  pathToFileURL(outFile).href
);

globalThis.isTauri = false;
assert.equal(isTauriInvokeUnavailable(new Error("any browser preview failure")), true);

globalThis.isTauri = true;
for (const message of [
  "Command load_pricing_comparison_workspace not found",
  "Command load_pricing_comparison_workspace not allowed by ACL",
  "Cannot read properties of undefined (reading '__TAURI_INTERNALS__')",
]) {
  assert.equal(isTauriInvokeUnavailable(new Error(message)), false, message);
}
