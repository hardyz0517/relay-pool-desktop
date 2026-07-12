import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stylesSource = await readFile("src/styles.css", "utf8");

function readKeyframes(name) {
  const start = stylesSource.indexOf(`@keyframes ${name}`);
  assert.notEqual(start, -1, `styles should define ${name} keyframes`);

  const nextKeyframes = stylesSource.indexOf("@keyframes", start + 1);
  const nextMedia = stylesSource.indexOf("@media", start + 1);
  const endCandidates = [nextKeyframes, nextMedia].filter((index) => index !== -1);
  const end = endCandidates.length > 0 ? Math.min(...endCandidates) : stylesSource.length;

  return stylesSource.slice(start, end);
}

assert.ok(
  stylesSource.includes(".app-page-transition-stack") &&
    stylesSource.includes("[data-page-transition-layer]") &&
    stylesSource.includes("min-height: 100%") &&
    stylesSource.includes('data-page-transition-state="active"') &&
    stylesSource.includes('data-page-transition-state="inactive"') &&
    stylesSource.includes('data-page-transition-state="exiting"'),
  "styles should define stack, full-height layer, active, inactive, and exiting selectors",
);

assert.ok(
  stylesSource.includes(
    '.app-page-transition-stack[data-page-transition-handoff="none"]',
  ) &&
    stylesSource.includes("relayPageFadeUp") &&
    stylesSource.includes("relayTransientEnter") &&
    stylesSource.includes("relayTransientExit"),
  "shell entry animation should run only for a fresh shell navigation",
);

assert.ok(
  stylesSource.includes('data-page-transition-handoff="transient-exit"') &&
    stylesSource.includes("animation: none"),
  "returning from a transient page should keep the parent shell visually stable during overlay exit",
);

const transientEnterKeyframes = readKeyframes("relayTransientEnter");
const transientExitKeyframes = readKeyframes("relayTransientExit");

assert.ok(
  !transientEnterKeyframes.includes("translateX") &&
    !transientExitKeyframes.includes("translateX") &&
    transientEnterKeyframes.includes("translateY") &&
    transientExitKeyframes.includes("translateY") &&
    transientEnterKeyframes.includes("scale(") &&
    transientExitKeyframes.includes("scale("),
  "transient page animations should feel like a soft fade/settle, not a horizontal slide",
);

assert.ok(
  stylesSource.includes("animation: relayTransientEnter 140ms") &&
    stylesSource.includes("animation: relayTransientExit 140ms") &&
    !transientEnterKeyframes.includes("opacity: 0;"),
  "transient navigation should stay immediately legible and finish quickly",
);

assert.ok(
  stylesSource.includes("pointer-events: none") &&
    stylesSource.includes("visibility: hidden") &&
    stylesSource.includes("display: none"),
  "inactive and exiting styles should isolate hidden pages from interaction",
);

assert.ok(
  stylesSource.includes("@media (prefers-reduced-motion: reduce)") &&
    stylesSource.includes("animation-duration: 1ms") &&
    stylesSource.includes("transition-duration: 1ms"),
  "styles should reduce motion when the system requests it",
);

console.log("page transition styles contract ok");
