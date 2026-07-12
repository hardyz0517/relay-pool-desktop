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
assertDeclaration(enteringRule, "animation", "relayShellPageEnter 140ms ease-out");
assertDeclaration(enteringRule, "will-change", "opacity");
for (const property of ["transition", "transform", "filter"]) {
  assertNoDeclaration(enteringRule, property);
}

assertDeclaration(overlayRule, "position", "absolute");
assertDeclaration(overlayRule, "inset", "0");
assertDeclaration(overlayRule, "z-index", "1");
assertDeclaration(overlayRule, "min-height", "100%");
assertDeclaration(overlayRule, "background", "hsl(var(--background))");
assertDeclaration(overlayRule, "pointer-events", "auto");
assertDeclaration(overlayRule, "will-change", "opacity");

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

const freshShellRule = readRule(
  '.app-page-transition-stack[data-page-transition-handoff="none"]\n' +
    '  .app-page-transition-layer[data-page-transition-kind="shell"][data-page-transition-state="active"]',
);
assertDeclaration(
  freshShellRule,
  "animation",
  "relayPageFadeUp 160ms ease-out",
);
readRule("@keyframes relayPageFadeUp");
readRule("@keyframes relayShellPageEnter");

const transientExitHandoffRule = readRule(
  '.app-page-transition-stack[data-page-transition-handoff="transient-exit"]\n' +
    '  .app-page-transition-layer[data-page-transition-kind="shell"][data-page-transition-state="active"]',
);
assertDeclaration(transientExitHandoffRule, "animation", "none");

const reducedMotionRule = readRule("@media (prefers-reduced-motion: reduce)");
const normalizedReducedMotionRule = reducedMotionRule
  .split("\n")
  .map((line) => (line.startsWith("  ") ? line.slice(2) : line))
  .join("\n");
const reducedMotionShellSelector =
  '.app-page-transition-layer[data-page-transition-kind="shell"]';
const reducedMotionShellRule = readRuleFrom(
  normalizedReducedMotionRule,
  reducedMotionShellSelector,
);

assertDeclaration(
  reducedMotionShellRule,
  "animation-duration",
  "1ms !important",
);
assertDeclaration(reducedMotionShellRule, "transform", "none !important");
const reducedMotionEnteringSelector =
  '.app-page-transition-layer[data-page-transition-state="entering"]';
const reducedMotionEnteringRule = readRuleFrom(
  normalizedReducedMotionRule,
  reducedMotionEnteringSelector,
);
assertDeclaration(
  reducedMotionEnteringRule,
  "animation-duration",
  "1ms !important",
);

console.log("page transition styles contract ok");
