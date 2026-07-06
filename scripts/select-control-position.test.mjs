import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/components/ui/SelectControl.tsx", "utf8");

assert.ok(
  source.includes("estimateMenuHeight"),
  "SelectControl should estimate rendered menu height before positioning an upward-opening menu",
);

assert.match(
  source,
  /const menuHeight = estimateMenuHeight\(options, maxHeight\);[\s\S]*rect\.top - menuHeight - gap/,
  "upward-opening SelectControl menus should use content height instead of maxHeight for top",
);

assert.ok(
  !source.includes("rect.top - maxHeight - gap"),
  "using maxHeight directly makes short dropdown menus float too far above the trigger",
);

assert.match(
  source,
  /const handleViewportResize = \(\) => updatePosition\(\);/,
  "SelectControl should reposition on viewport resize",
);

assert.match(
  source,
  /const handleViewportScroll = \(event: Event\) => \{[\s\S]*setOpen\(false\);[\s\S]*\};/,
  "SelectControl should close on page/container scroll instead of trying to follow with delayed fixed-position updates",
);

assert.ok(
  !source.includes('window.addEventListener("scroll", handleViewportChange, true)'),
  "scroll should not keep a fixed portal menu open and chase its trigger",
);

assert.ok(
  source.includes('window.addEventListener("wheel", handleViewportScroll'),
  "mouse-wheel scrolling should close the menu before the fixed portal can visibly lag behind",
);
