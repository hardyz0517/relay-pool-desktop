import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/components/StationGroupChip.tsx", "utf8");

assert.ok(
  source.includes("StationGroupInlineBadge"),
  "station group chip module should expose a single inline badge for select labels",
);

const optionLabelStart = source.indexOf("export function StationGroupOptionLabel");
assert.notEqual(optionLabelStart, -1, "StationGroupOptionLabel should exist");
const optionLabelBody = source.slice(optionLabelStart);

assert.ok(
  optionLabelBody.includes("<StationGroupInlineBadge"),
  "station group select labels should render group name and multiplier as one inline chip",
);
assert.ok(
  !optionLabelBody.includes("<StationGroupNameBadge") && !optionLabelBody.includes("<StationGroupRateBadge"),
  "station group select labels should not render separate name and rate pills",
);
assert.ok(
  source.includes(
    '"inline-flex h-6 max-w-full items-center gap-2 rounded-md px-2 text-xs font-medium"',
  ),
  "the inline chip should use one compact, borderless outer pill with balanced spacing",
);
assert.ok(
  source.includes(
    '"inline-flex h-5 shrink-0 items-center rounded-md bg-black/10 px-1.5 text-[10px] font-semibold leading-none"',
  ),
  "the multiplier should be a smaller rounded inset inside the outer pill",
);
assert.ok(
  source.includes('visualMeta.platform === "openai" ? "bg-green-50 text-green-700"') &&
    source.includes('visualMeta.platform === "openai" ? "text-green-700"'),
  "the OpenAI inline chip should match the green palette used by the reference design",
);
assert.ok(
  !source.includes("items-stretch overflow-hidden") && !source.includes("border-l border-white/70"),
  "the multiplier should not be rendered as a full-height split segment",
);
