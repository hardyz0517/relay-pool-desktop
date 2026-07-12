import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/components/StationDetailContent.tsx", "utf8");

assert.ok(
  source.includes("usageCardVisualMeta") &&
    source.includes("request:") &&
    source.includes("consumption:") &&
    source.includes("todayToken:") &&
    source.includes("totalToken:"),
  "station detail usage cards should define fixed visual accents for each metric type",
);

assert.ok(
  source.includes("bg-green-100 text-green-700") &&
    source.includes("bg-purple-100 text-purple-700") &&
    source.includes("bg-amber-100 text-amber-700") &&
    source.includes("bg-indigo-100 text-indigo-700"),
  "station detail usage cards should mirror dashboard metric accent colors",
);

assert.ok(
  source.includes("flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px]") &&
    source.includes("text-[22px] font-semibold leading-7"),
  "station detail usage cards should use dashboard-like icon blocks and value sizing",
);

assert.ok(
  source.includes("usageCardVisualFor(card.label)") &&
    source.includes("<visual.Icon className=\"h-4 w-4\" />"),
  "station detail usage cards should render per-metric icons",
);
