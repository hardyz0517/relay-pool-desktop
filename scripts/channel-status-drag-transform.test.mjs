import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/channels/ChannelStatusTab.tsx", "utf8");

assert.ok(
  !source.includes("toActiveChannelDragTransform"),
  "channel status sorting should not clamp the active card transform because that blocks vertical drag movement",
);

assert.ok(
  !source.includes("modifiers={[restrictChannelDragToContainer]}"),
  "DndContext modifiers also affect sortable displacement transforms and can break vertical reordering",
);

assert.ok(
  source.includes("transform: CSS.Transform.toString(transform)"),
  "sortable cards need dnd-kit's raw two-dimensional transform for horizontal and vertical grid sorting",
);

assert.ok(
  !source.includes("x: 0"),
  "channel status grid sorting must not zero horizontal movement",
);

assert.ok(
  !source.includes("clamp(transform."),
  "channel status grid sorting must not clamp transform axes locally",
);
