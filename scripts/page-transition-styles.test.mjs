import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stylesSource = (await readFile("src/styles.css", "utf8")).replace(
  /\r\n?/g,
  "\n",
);

function readRuleFrom(source, selector) {
  const opening = `${selector} {`;
  const lineStart = source.indexOf(`\n${opening}`);
  const ruleStart = source.startsWith(opening)
    ? 0
    : lineStart === -1
      ? -1
      : lineStart + 1;

  assert.notEqual(ruleStart, -1, `styles should define exact rule ${selector}`);

  const bodyStart = ruleStart + opening.length;
  let depth = 1;

  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") {
      depth += 1;
    } else if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) {
        return source.slice(bodyStart, index);
      }
    }
  }

  assert.fail(`rule ${selector} should have a closing brace`);
}

function readRule(selector) {
  return readRuleFrom(stylesSource, selector);
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function assertDeclaration(ruleBody, property, value) {
  assert.match(
    ruleBody,
    new RegExp(
      `^\\s*${escapeRegExp(property)}:\\s*${escapeRegExp(value)};\\s*$`,
      "m",
    ),
    `expected ${property}: ${value}; in exact rule body`,
  );
}

function assertNoDeclaration(ruleBody, property) {
  assert.doesNotMatch(
    ruleBody,
    new RegExp(`^\\s*${escapeRegExp(property)}\\s*:`, "m"),
    `exact rule body should not declare ${property}`,
  );
}

const stackRule = readRule(".app-page-transition-stack");
const baseLayerRule = readRule(
  ".app-page-transition-layer,\n[data-page-transition-layer]",
);
const shellLayerRule = readRule(
  '.app-page-transition-layer[data-page-transition-kind="shell"]',
);
const backgroundRule = readRule(
  '.app-page-transition-layer[data-page-transition-state="background"]',
);
const inactiveRule = readRule(
  '.app-page-transition-layer[data-page-transition-state="inactive"]',
);
const activeRule = readRule(
  '.app-page-transition-layer[data-page-transition-state="active"]',
);
const enteringRule = readRule(
  '.app-page-transition-layer[data-page-transition-state="entering"]',
);
const overlayRule = readRule(".app-page-transition-overlay");
const contentRule = readRule(".app-page-transition-content");

assertDeclaration(stackRule, "position", "relative");
assertDeclaration(stackRule, "height", "100%");
assertDeclaration(stackRule, "min-height", "100%");
assertDeclaration(stackRule, "isolation", "isolate");
assertDeclaration(baseLayerRule, "height", "100%");
assertDeclaration(baseLayerRule, "min-height", "100%");
assertDeclaration(baseLayerRule, "min-width", "0");
assertDeclaration(baseLayerRule, "overflow-y", "auto");

for (const property of ["transform", "opacity", "transition", "animation"]) {
  assertNoDeclaration(baseLayerRule, property);
}

assertDeclaration(shellLayerRule, "position", "relative");
assertDeclaration(shellLayerRule, "z-index", "0");
assertDeclaration(shellLayerRule, "isolation", "isolate");

for (const property of ["transform", "opacity", "transition", "animation"]) {
  assertNoDeclaration(shellLayerRule, property);
}

assertDeclaration(backgroundRule, "display", "block");
assertDeclaration(backgroundRule, "visibility", "visible");
assertDeclaration(backgroundRule, "pointer-events", "none");

assertDeclaration(inactiveRule, "display", "none");
assertDeclaration(inactiveRule, "visibility", "hidden");
assertDeclaration(inactiveRule, "pointer-events", "none");

assertDeclaration(activeRule, "display", "block");
assertDeclaration(activeRule, "visibility", "visible");
assertDeclaration(activeRule, "pointer-events", "auto");
assertDeclaration(enteringRule, "position", "absolute");
assertDeclaration(enteringRule, "inset", "0");
assertDeclaration(enteringRule, "z-index", "1");
assertDeclaration(enteringRule, "display", "block");
assertDeclaration(enteringRule, "visibility", "visible");
assertDeclaration(enteringRule, "pointer-events", "auto");
assertDeclaration(enteringRule, "background", "hsl(var(--background))");
for (const property of ["animation", "transition", "transform", "filter", "opacity", "will-change"]) {
  assertNoDeclaration(enteringRule, property);
}

assertDeclaration(overlayRule, "position", "absolute");
assertDeclaration(overlayRule, "inset", "0");
assertDeclaration(overlayRule, "z-index", "1");
assertDeclaration(overlayRule, "min-height", "100%");
assertDeclaration(overlayRule, "background", "hsl(var(--background))");
assertDeclaration(overlayRule, "pointer-events", "auto");
assertNoDeclaration(overlayRule, "will-change");

for (const property of ["animation", "transition", "transform"]) {
  assertNoDeclaration(overlayRule, property);
}

assertDeclaration(contentRule, "min-height", "100%");

assert.equal(
  stylesSource.includes("relayTransientEnter"),
  false,
  "relayTransientEnter should not occur in transition CSS",
);
assert.equal(
  stylesSource.includes("relayTransientExit"),
  false,
  "relayTransientExit should not occur in transition CSS",
);
assert.doesNotMatch(
  stylesSource,
  /\[data-page-transition-direction(?:=|\])/,
  "direction-specific transition selectors should not remain",
);

assert.equal(
  stylesSource.includes("relayPageFadeUp"),
  false,
  "a completed shell handoff should not restart a second page-level fade",
);
assert.equal(
  stylesSource.includes("relayShellPageEnter"),
  false,
  "the entering shell layer should stay opaque so the previous page cannot bleed through",
);
assert.equal(
  stylesSource.includes("relayShellPageContentEnter"),
  false,
  "shell content motion should be owned by the centralized Motion host",
);
assert.equal(
  stylesSource.includes("@media (prefers-reduced-motion: reduce)"),
  false,
  "page motion reduction should be centralized in MotionConfig",
);

console.log("page transition styles contract ok");
