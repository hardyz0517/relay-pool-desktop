import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
const esbuild = require("../node_modules/.pnpm/node_modules/esbuild");

const outFile = resolve(tmpdir(), "relay-pool-channel-status-view-model.test.mjs");
await mkdir(dirname(outFile), { recursive: true });
await esbuild.build({
  entryPoints: ["src/features/channels/channelStatusViewModel.ts"],
  outfile: outFile,
  bundle: true,
  platform: "node",
  format: "esm",
  external: ["react", "lucide-react", "@tauri-apps/api/core"],
});

const {
  availabilityToneClassName,
  buildRecentOutcomes,
  orderChannelsBySavedOrder,
} = await import(pathToFileURL(outFile).href);

assert.equal(
  availabilityToneClassName({ status: "healthy", availabilityPercent: 50 }),
  "text-orange-600",
  "50% availability should be orange, not red",
);

const outcomes = buildRecentOutcomes([], {
  successCount: 2,
  failureCount: 2,
});
assert.equal(outcomes.length, 60, "outcome strip should keep 60 slots");
assert.equal(outcomes.filter((item) => item === "success").length, 2);
assert.equal(outcomes.filter((item) => item === "failed").length, 2);
assert.equal(outcomes.at(-1), "success", "latest health success should color the newest slot");

const orderedChannels = orderChannelsBySavedOrder(
  [
    { id: "new-channel", name: "new" },
    { id: "second", name: "second" },
    { id: "first", name: "first" },
  ],
  ["first", "second", "missing"],
);
assert.deepEqual(
  orderedChannels.map((channel) => channel.id),
  ["first", "second", "new-channel"],
  "saved channel order should be preserved while new channels append at the end",
);
