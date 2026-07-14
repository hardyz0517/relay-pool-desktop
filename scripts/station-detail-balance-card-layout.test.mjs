import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/components/StationDetailContent.tsx", "utf8");

assert.ok(
  source.includes("balanceCardVisualMeta") &&
    source.includes("current:") &&
    source.includes("threshold:") &&
    source.includes("updatedAt:") &&
    source.includes("concurrency:"),
  "station detail balance cards should define fixed visual accents for each balance metric type",
);

assert.ok(
  source.includes("balanceCardVisualFor(card.label)") && source.includes("<visual.Icon className=\"h-4 w-4\" />"),
  "station detail balance cards should render per-metric icons",
);

assert.ok(
  source.includes("flex min-h-[96px] items-center gap-3 rounded-[12px] border border-border bg-surface px-4 py-3 shadow-surface"),
  "station detail balance cards should use the same card shell as usage cards",
);

assert.ok(
  source.includes("grid-cols-[repeat(auto-fit,minmax(180px,1fr))]"),
  "station detail balance cards should keep three or four cards on one row when the section has enough width",
);

assert.ok(
  source.includes("flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]") &&
    source.includes("text-[22px] font-semibold leading-7"),
  "station detail balance cards should use dashboard-like icon blocks and value sizing",
);
