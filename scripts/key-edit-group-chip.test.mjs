import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const sharedChipSource = await readFile("src/features/stations/components/StationGroupChip.tsx", "utf8");
const stationDetailSource = await readFile("src/features/stations/components/StationDetailContent.tsx", "utf8");
const editKeySource = await readFile("src/features/key-pool/EditKeyPage.tsx", "utf8");
const keyPoolSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");

assert.ok(
  sharedChipSource.includes("StationGroupNameBadge") &&
    sharedChipSource.includes("StationGroupRateBadge") &&
    sharedChipSource.includes("StationGroupOptionLabel"),
  "station group chip module should expose reusable name, rate, and option label components",
);

assert.ok(
  sharedChipSource.includes("groupVisualMetaFor") &&
    sharedChipSource.includes("visualMeta.badgeClassName") &&
    sharedChipSource.includes("visualMeta.rateBadgeClassName"),
  "station group chip styles should come from groupVisualMetaFor instead of page-local hard-coded colors",
);

for (const [sourceName, source] of [
  ["station detail", stationDetailSource],
  ["edit-key page", editKeySource],
  ["key-pool edit dialog", keyPoolSource],
]) {
  assert.ok(
    source.includes("StationGroupNameBadge") || source.includes("StationGroupOptionLabel"),
    `${sourceName} should render group names through the shared station group chip`,
  );
}

for (const [sourceName, source] of [
  ["edit-key page", editKeySource],
  ["key-pool edit dialog", keyPoolSource],
]) {
  assert.ok(
    source.includes("<StationGroupOptionLabel"),
    `${sourceName} group select options should show the station-style group chip and multiplier chip`,
  );
  assert.ok(
    !source.includes("`${sourceItem.groupName ?? \"当前绑定\"} · ${formatRate(sourceItem.rateMultiplier)} · 当前`"),
    `${sourceName} should not fall back to plain text current-group multiplier labels`,
  );
  assert.ok(
    /function currentGroupOption[\s\S]*findMatchingGroupOption\(/.test(source) &&
      !/function currentGroupOption[\s\S]*options\.some\(\(option\) => option\.groupBindingId === sourceItem\.groupBindingId/.test(source),
    `${sourceName} should suppress current-only fallback options when the current binding matches a selectable canonical station group`,
  );
}
