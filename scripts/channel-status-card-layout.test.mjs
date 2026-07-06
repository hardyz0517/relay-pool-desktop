import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8");

assert.ok(
  source.includes("sm:grid-cols-[repeat(auto-fill,minmax(320px,360px))]"),
  "channel status cards should keep a bounded desktop width instead of stretching with the page",
);

assert.ok(
  source.includes("grid-cols-1"),
  "channel status cards should remain a single full-width column on narrow screens",
);

assert.ok(
  source.includes("justify-start"),
  "channel status grid should leave extra wide-screen space at the end instead of stretching cards",
);

assert.ok(
  !source.includes("md:grid-cols-2 2xl:grid-cols-3"),
  "fractional breakpoint columns make channel status cards grow too wide on large pages",
);

assert.ok(
  !source.includes('{channel.lastError ?? ""}'),
  "channel status cards should not render the last error summary as a bottom line because it changes card height",
);
