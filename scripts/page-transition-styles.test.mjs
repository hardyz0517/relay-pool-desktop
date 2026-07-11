import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stylesSource = await readFile("src/styles.css", "utf8");

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
  stylesSource.includes("relayPageFadeUp") &&
    stylesSource.includes("relayTransientEnter") &&
    stylesSource.includes("relayTransientExit"),
  "styles should define shell fade-up and transient enter/exit animations",
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
