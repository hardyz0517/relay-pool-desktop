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

const { classifyTauriInvokeError, isTauriCommandNotFound, isTauriInvokeUnavailable } = await import(
  pathToFileURL(outFile).href
);

const cases = [
  ["Command load_pricing_comparison_workspace not found", "command-not-found"],
  ["Command load_channel_status_workspace not found", "command-not-found"],
  ["Command mark_change_events_read not found", "command-not-found"],
  ["Command load_pricing_comparison_workspace not allowed by ACL", "acl-denied"],
  ["database record not found", "other"],
  ["Cannot read properties of undefined (reading '__TAURI_INTERNALS__')", "runtime-unavailable"],
];

for (const [message, expected] of cases) {
  assert.equal(classifyTauriInvokeError(new Error(message)), expected, message);
}

assert.equal(
  isTauriCommandNotFound(new Error("Command load_pricing_comparison_workspace not allowed by ACL")),
  false,
);

assert.equal(isTauriInvokeUnavailable(new Error("Command mark_change_events_read not found")), false);
