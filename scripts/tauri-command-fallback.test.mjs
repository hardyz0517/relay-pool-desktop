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

const { isTauriCommandNotFound } = await import(pathToFileURL(outFile).href);

for (const message of [
  "load_pricing_comparison_workspace not allowed. Command not found",
  "load_channel_status_workspace not allowed. Command not found",
  "mark_change_events_read not allowed. Command not found",
  "Command mark_change_events_read not found",
]) {
  assert.equal(
    isTauriCommandNotFound(new Error(message)),
    true,
    `should recognize a Tauri command-missing response: ${message}`,
  );
}

assert.equal(
  isTauriCommandNotFound(new Error("database record not found")),
  false,
  "unrelated not-found errors must not trigger a legacy command fallback",
);
