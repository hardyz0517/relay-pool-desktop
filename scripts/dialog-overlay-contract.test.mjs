import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dialogSource = await readFile("src/components/ui/Dialog.tsx", "utf8");

assert.ok(
  dialogSource.includes('import { createPortal } from "react-dom";') &&
    dialogSource.includes("return createPortal(") &&
    dialogSource.includes("document.body"),
  "Dialog should render through a body portal so fixed modals are not trapped inside page transition or scroll containers",
);

assert.ok(
  dialogSource.includes(
    'import { useInteractionActivity } from "@/components/ui/InteractionActivity";',
  ) && dialogSource.includes("const interactionActive = useInteractionActivity();"),
  "Dialog should consume the shared interaction activity that owns body portals",
);

assert.match(
  dialogSource,
  /useLayoutEffect\(\(\) => \{\s*if \(!interactionActive && open\) \{\s*onClose\(\);\s*\}\s*\}, \[interactionActive, onClose, open\]\);/,
  "Dialog should close its controlled open state in the committed layout phase when its page becomes inactive",
);

assert.ok(
  dialogSource.includes("if (!open || !interactionActive) {") &&
    dialogSource.includes("[interactionActive, open]") &&
    /if \(!open \|\| !interactionActive\) \{\s*return null;\s*\}/.test(dialogSource),
  "Dialog should stop rendering and release its body lock as soon as interaction becomes inactive",
);

assert.ok(
  dialogSource.includes("document.body.style.overflow = \"hidden\"") &&
    dialogSource.includes("document.body.style.overflow = previousBodyOverflow"),
  "Dialog should lock body scrolling while open and restore the previous body overflow on close",
);

assert.ok(
  dialogSource.includes("bg-white/35") &&
    dialogSource.includes("backdrop-blur-[1px]") &&
    !dialogSource.includes("bg-slate-900/20") &&
    !dialogSource.includes("backdrop-blur-[2px]"),
  "Dialog should use a light desktop-tool veil instead of making the background gray",
);

console.log("dialog overlay contract ok");
