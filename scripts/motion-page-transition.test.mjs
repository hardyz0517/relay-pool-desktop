import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import ts from "typescript";

function isFramerMotionModuleName(moduleName) {
  return moduleName === "framer-motion" || moduleName.startsWith("framer-motion/");
}

function isFramerMotionSpecifier(node) {
  return ts.isStringLiteralLike(node) && isFramerMotionModuleName(node.text);
}

function sourceReferencesFramerMotion(source, fileName = "fixture.tsx") {
  const sourceFile = ts.createSourceFile(
    fileName,
    source,
    ts.ScriptTarget.Latest,
    true,
  );
  let found = false;

  function visit(node) {
    const declarationReference =
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier !== undefined &&
      isFramerMotionSpecifier(node.moduleSpecifier);
    const callReference =
      ts.isCallExpression(node) &&
      (node.expression.kind === ts.SyntaxKind.ImportKeyword ||
        (ts.isIdentifier(node.expression) && node.expression.text === "require")) &&
      node.arguments.length > 0 &&
      isFramerMotionSpecifier(node.arguments[0]);

    if (declarationReference || callReference) {
      found = true;
      return;
    }
    ts.forEachChild(node, visit);
  }

  visit(sourceFile);
  return found;
}

const framerMotionReferenceFixtures = [
  ["side-effect import", 'import "framer-motion";'],
  ["line-broken import", 'import {\n  motion,\n} from\n  "framer-motion";'],
  ["re-export", 'export { motion } from "framer-motion";'],
  ["dynamic import", 'const motionModule = import("framer-motion");'],
  ["require call", 'const motionModule = require("framer-motion");'],
  ["m subpath", 'import { m } from "framer-motion/m";'],
  ["client subpath", 'export { MotionConfig } from "framer-motion/client";'],
];

assert.deepEqual(
  framerMotionReferenceFixtures
    .filter(([, source]) => !sourceReferencesFramerMotion(source))
    .map(([label]) => label),
  [],
  "the module scanner should detect every supported framer-motion reference",
);

const nonFramerMotionReferenceFixtures = [
  ["similarly named package", 'import "framer-motion-extra";'],
  ["scoped package", 'import "@scope/framer-motion";'],
];

assert.deepEqual(
  nonFramerMotionReferenceFixtures
    .filter(([, source]) => sourceReferencesFramerMotion(source))
    .map(([label]) => label),
  [],
  "the module scanner should reject packages outside the framer-motion boundary",
);

const forbiddenMotionPropertyNames = new Set([
  "x",
  "y",
  "scale",
  "filter",
  "backdropFilter",
]);

function objectLiteralPropertyName(node) {
  if (ts.isShorthandPropertyAssignment(node)) {
    return node.name.text;
  }
  if (
    ts.isPropertyAssignment(node) &&
    (ts.isIdentifier(node.name) || ts.isStringLiteralLike(node.name))
  ) {
    return node.name.text;
  }
  return undefined;
}

function findForbiddenObjectLiteralProperties(source, fileName = "fixture.tsx") {
  const sourceFile = ts.createSourceFile(
    fileName,
    source,
    ts.ScriptTarget.Latest,
    true,
  );
  const forbiddenProperties = [];

  function visit(node) {
    const propertyName = objectLiteralPropertyName(node);
    if (
      propertyName !== undefined &&
      forbiddenMotionPropertyNames.has(propertyName)
    ) {
      forbiddenProperties.push(propertyName);
    }
    ts.forEachChild(node, visit);
  }

  visit(sourceFile);
  return forbiddenProperties;
}

function parseTsxSource(source, fileName = "fixture.tsx") {
  return ts.createSourceFile(
    fileName,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX,
  );
}

function findNodes(root, predicate) {
  const matches = [];

  function visit(node) {
    if (predicate(node)) {
      matches.push(node);
    }
    ts.forEachChild(node, visit);
  }

  visit(root);
  return matches;
}

function isDocumentBody(node) {
  return (
    ts.isPropertyAccessExpression(node) &&
    ts.isIdentifier(node.expression) &&
    node.expression.text === "document" &&
    node.name.text === "body"
  );
}

function isCreatePortalCallee(node) {
  return (
    (ts.isIdentifier(node) && node.text === "createPortal") ||
    (ts.isPropertyAccessExpression(node) && node.name.text === "createPortal")
  );
}

function findBodyPortalCalls(sourceFile) {
  return findNodes(
    sourceFile,
    (node) =>
      ts.isCallExpression(node) &&
      isCreatePortalCallee(node.expression) &&
      node.arguments.length >= 2 &&
      isDocumentBody(node.arguments[1]),
  );
}

function interactionActivityHookNames(sourceFile) {
  const hookNames = new Set();
  for (const node of findNodes(sourceFile, ts.isImportDeclaration)) {
    if (
      !ts.isStringLiteralLike(node.moduleSpecifier) ||
      node.moduleSpecifier.text !== "@/components/ui/InteractionActivity" ||
      !node.importClause?.namedBindings ||
      !ts.isNamedImports(node.importClause.namedBindings)
    ) {
      continue;
    }
    for (const element of node.importClause.namedBindings.elements) {
      if ((element.propertyName ?? element.name).text === "useInteractionActivity") {
        hookNames.add(element.name.text);
      }
    }
  }
  return hookNames;
}

function consumesInteractionActivity(sourceFile) {
  const hookNames = interactionActivityHookNames(sourceFile);
  return findNodes(
    sourceFile,
    (node) =>
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      hookNames.has(node.expression.text),
  ).length > 0;
}

function findJsxOpeningElements(sourceFile, tagName) {
  return findNodes(
    sourceFile,
    (node) =>
      (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) &&
      ts.isIdentifier(node.tagName) &&
      node.tagName.text === tagName,
  );
}

function getJsxAttribute(openingElement, attributeName) {
  return openingElement.attributes.properties.find(
    (property) =>
      ts.isJsxAttribute(property) &&
      ts.isIdentifier(property.name) &&
      property.name.text === attributeName,
  );
}

const forbiddenPropertyFixture = `
  const scale = 1;
  const backdropFilter = "blur(2px)";
  const transition = {
    "x": 24,
    'y': 12,
    scale,
    filter: "blur(1px)",
    backdropFilter,
  };
`;

assert.deepEqual(
  findForbiddenObjectLiteralProperties(forbiddenPropertyFixture),
  ["x", "y", "scale", "filter", "backdropFilter"],
  "the property scanner should detect quoted and shorthand forbidden keys",
);

async function readSourceFiles(root) {
  const entries = await readdir(root, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const entryPath = path.join(root, entry.name);
      if (entry.isDirectory()) {
        return readSourceFiles(entryPath);
      }
      return /\.[cm]?[jt]sx?$/.test(entry.name) ? [entryPath] : [];
    }),
  );
  return nested.flat();
}

const packageJson = JSON.parse(await readFile("package.json", "utf8"));
const hostPath = path.normalize("src/app/TransientPageHost.tsx");
const hostSource = await readFile(hostPath, "utf8");
const hostSourceFile = parseTsxSource(hostSource, hostPath);
const interactionActivitySource = await readFile(
  "src/components/ui/InteractionActivity.tsx",
  "utf8",
);
const pageActivitySource = await readFile(
  "src/components/shell/PageActivity.tsx",
  "utf8",
);
const selectControlSource = await readFile(
  "src/components/ui/SelectControl.tsx",
  "utf8",
);
const sourceFiles = await readSourceFiles("src");
const motionImporters = [];
const bodyPortalCallSources = [];
const bodyPortalCallSourcesWithoutActivity = [];

for (const sourcePath of sourceFiles) {
  const source = await readFile(sourcePath, "utf8");
  if (sourceReferencesFramerMotion(source, sourcePath)) {
    motionImporters.push(path.normalize(sourcePath));
  }
  const sourceFile = parseTsxSource(source, sourcePath);
  const bodyPortalCalls = findBodyPortalCalls(sourceFile);
  if (bodyPortalCalls.length > 0) {
    const normalizedSourcePath = path.normalize(sourcePath);
    bodyPortalCallSources.push(
      ...bodyPortalCalls.map(() => normalizedSourcePath),
    );
    if (!consumesInteractionActivity(sourceFile)) {
      bodyPortalCallSourcesWithoutActivity.push(
        ...bodyPortalCalls.map(() => normalizedSourcePath),
      );
    }
  }
}

assert.equal(
  packageJson.dependencies?.["framer-motion"],
  "^12.23.25",
  "framer-motion should be a pinned runtime dependency",
);
assert.ok(
  interactionActivitySource.includes("createContext(true)") &&
    interactionActivitySource.includes("export function InteractionActivityProvider") &&
    interactionActivitySource.includes(
      "<InteractionActivityContext.Provider value={active}>",
    ) &&
    interactionActivitySource.includes("export function useInteractionActivity()") &&
    interactionActivitySource.includes(
      "return useContext(InteractionActivityContext);",
    ),
  "interaction activity should expose a default-active shared context",
);
assert.ok(
  pageActivitySource.includes("<InteractionActivityProvider active={active}>") &&
    pageActivitySource.includes("{children}") &&
    pageActivitySource.includes("</InteractionActivityProvider>") &&
    pageActivitySource.includes("const interactive = useInteractionActivity();") &&
    pageActivitySource.includes("refreshEnabled: boolean") &&
    pageActivitySource.includes("export function usePageActivity()"),
  "page activity should share one interaction-active state without changing activation semantics",
);
assert.ok(
  selectControlSource.includes(
    "const interactionActive = useInteractionActivity();",
  ) &&
    /useLayoutEffect\(\(\) => \{\s*if \(interactionActive\) \{\s*return;\s*\}\s*setOpen\(false\);\s*setPosition\(null\);\s*\}, \[interactionActive\]\);/.test(
      selectControlSource,
    ),
  "select menus should close synchronously when their owning page becomes inactive",
);
assert.deepEqual(
  bodyPortalCallSources.sort(),
  [
    path.normalize("src/components/ui/Dialog.tsx"),
    path.normalize("src/components/ui/SelectControl.tsx"),
    path.normalize("src/features/pricing/ModelBasePricesPage.tsx"),
  ].sort(),
  "the portal audit should enumerate every current document.body portal call",
);
assert.deepEqual(
  bodyPortalCallSourcesWithoutActivity,
  [],
  "every document.body portal source should consume interaction activity",
);
assert.deepEqual(
  motionImporters,
  [hostPath],
  "TransientPageHost should be the only source module importing framer-motion",
);
assert.doesNotMatch(
  hostSource,
  /\bcloneElement\b/,
  "the Motion host should declare presence lifecycle props directly",
);

const motionConfigElements = findJsxOpeningElements(hostSourceFile, "MotionConfig");
assert.equal(
  motionConfigElements.length,
  1,
  "the host should render exactly one MotionConfig boundary",
);
const reducedMotionAttribute = getJsxAttribute(
  motionConfigElements[0],
  "reducedMotion",
);
assert.ok(
  reducedMotionAttribute?.initializer &&
    ts.isStringLiteral(reducedMotionAttribute.initializer) &&
    reducedMotionAttribute.initializer.text === "user",
  "MotionConfig should delegate reduced-motion behavior to the user preference",
);

const animatePresenceElements = findJsxOpeningElements(
  hostSourceFile,
  "AnimatePresence",
);
assert.equal(
  animatePresenceElements.length,
  1,
  "the host should render exactly one AnimatePresence boundary",
);
const animatePresenceElement = animatePresenceElements[0];
const initialAttribute = getJsxAttribute(animatePresenceElement, "initial");
const modeAttribute = getJsxAttribute(animatePresenceElement, "mode");
const exitCompleteAttribute = getJsxAttribute(
  animatePresenceElement,
  "onExitComplete",
);
const exitCompleteExpression =
  exitCompleteAttribute?.initializer &&
  ts.isJsxExpression(exitCompleteAttribute.initializer)
    ? exitCompleteAttribute.initializer.expression
    : undefined;
assert.ok(
  initialAttribute?.initializer &&
    ts.isJsxExpression(initialAttribute.initializer) &&
    initialAttribute.initializer.expression?.kind === ts.SyntaxKind.FalseKeyword &&
    modeAttribute?.initializer &&
    ts.isStringLiteral(modeAttribute.initializer) &&
    modeAttribute.initializer.text === "wait" &&
    ts.isIdentifier(exitCompleteExpression),
  "the host should directly declare reduced-motion, wait-mode, and exit completion behavior",
);

const stableHandlerDeclarations = findNodes(
  hostSourceFile,
  (node) =>
    ts.isVariableDeclaration(node) &&
    ts.isIdentifier(node.name) &&
    node.name.text === exitCompleteExpression.text,
);
assert.equal(
  stableHandlerDeclarations.length,
  1,
  "AnimatePresence onExitComplete should resolve to one local stable handler",
);
const stableHandlerInitializer = stableHandlerDeclarations[0].initializer;
assert.ok(
  stableHandlerInitializer &&
    ts.isCallExpression(stableHandlerInitializer) &&
    ts.isIdentifier(stableHandlerInitializer.expression) &&
    stableHandlerInitializer.expression.text === "useCallback" &&
    (ts.isArrowFunction(stableHandlerInitializer.arguments[0]) ||
      ts.isFunctionExpression(stableHandlerInitializer.arguments[0])) &&
    ts.isArrayLiteralExpression(stableHandlerInitializer.arguments[1]) &&
    stableHandlerInitializer.arguments[1].elements.length === 0,
  "the exit completion handler should keep stable identity with useCallback([])",
);

const stableHandlerFunction = stableHandlerInitializer.arguments[0];
const exitPolicyCalls = findNodes(
  stableHandlerFunction,
  (node) =>
    ts.isCallExpression(node) &&
    ts.isIdentifier(node.expression) &&
    node.expression.text === "completeTransientPageExit",
);
assert.equal(
  exitPolicyCalls.length,
  1,
  "the stable handler should delegate once to the transient exit policy",
);
const latestSnapshotArgument = exitPolicyCalls[0].arguments[0];
assert.ok(
  exitPolicyCalls[0].arguments.length === 1 &&
    ts.isPropertyAccessExpression(latestSnapshotArgument) &&
    ts.isIdentifier(latestSnapshotArgument.expression) &&
    latestSnapshotArgument.name.text === "current",
  "the stable handler should read one latest committed snapshot ref",
);

const snapshotRefName = latestSnapshotArgument.expression.text;
const snapshotRefDeclarations = findNodes(
  hostSourceFile,
  (node) =>
    ts.isVariableDeclaration(node) &&
    ts.isIdentifier(node.name) &&
    node.name.text === snapshotRefName &&
    node.initializer &&
    ts.isCallExpression(node.initializer) &&
    ts.isIdentifier(node.initializer.expression) &&
    node.initializer.expression.text === "useRef",
);
assert.equal(
  snapshotRefDeclarations.length,
  1,
  "the latest exit snapshot should be owned by one ref",
);

const snapshotAssignments = findNodes(
  hostSourceFile,
  (node) =>
    ts.isBinaryExpression(node) &&
    node.operatorToken.kind === ts.SyntaxKind.EqualsToken &&
    ts.isPropertyAccessExpression(node.left) &&
    ts.isIdentifier(node.left.expression) &&
    node.left.expression.text === snapshotRefName &&
    node.left.name.text === "current",
);
assert.equal(
  snapshotAssignments.length,
  1,
  "the latest exit snapshot ref should have one committed update path",
);
const committedSnapshotValue = snapshotAssignments[0].right;
assert.ok(
  ts.isObjectLiteralExpression(committedSnapshotValue),
  "the committed exit snapshot should be an object literal",
);
const hasActivePageProperty = committedSnapshotValue.properties.find(
  (property) => objectLiteralPropertyName(property) === "hasActivePage",
);
assert.ok(
  hasActivePageProperty &&
    ts.isPropertyAssignment(hasActivePageProperty) &&
    ts.isBinaryExpression(hasActivePageProperty.initializer) &&
    ts.isIdentifier(hasActivePageProperty.initializer.left) &&
    hasActivePageProperty.initializer.left.text === "page" &&
    hasActivePageProperty.initializer.operatorToken.kind ===
      ts.SyntaxKind.ExclamationEqualsEqualsToken &&
    hasActivePageProperty.initializer.right.kind === ts.SyntaxKind.NullKeyword,
  "the committed snapshot should derive hasActivePage from page !== null",
);
const onExitCompleteProperty = committedSnapshotValue.properties.find(
  (property) => objectLiteralPropertyName(property) === "onExitComplete",
);
const committedExitCallback = onExitCompleteProperty
  ? ts.isShorthandPropertyAssignment(onExitCompleteProperty)
    ? onExitCompleteProperty.name
    : ts.isPropertyAssignment(onExitCompleteProperty)
      ? onExitCompleteProperty.initializer
      : undefined
  : undefined;
assert.ok(
  ts.isIdentifier(committedExitCallback) &&
    committedExitCallback.text === "onExitComplete",
  "the committed snapshot should carry the current host exit callback",
);

const layoutEffectCalls = findNodes(
  hostSourceFile,
  (node) =>
    ts.isCallExpression(node) &&
    ts.isIdentifier(node.expression) &&
    node.expression.text === "useLayoutEffect",
);
const committedSnapshotEffects = layoutEffectCalls.filter((effectCall) =>
  findNodes(effectCall.arguments[0], (node) => node === snapshotAssignments[0])
    .length > 0,
);
assert.equal(
  committedSnapshotEffects.length,
  1,
  "the latest exit snapshot should update only after React commits a layout effect",
);
const snapshotEffectDependencies = committedSnapshotEffects[0].arguments[1];
assert.ok(
  ts.isArrayLiteralExpression(snapshotEffectDependencies) &&
    snapshotEffectDependencies.elements.some(
      (element) => ts.isIdentifier(element) && element.text === "page",
    ) &&
    snapshotEffectDependencies.elements.some(
      (element) => ts.isIdentifier(element) && element.text === "onExitComplete",
    ),
  "the committed snapshot should update with both current page and host callback",
);
assert.ok(
  hostSource.includes("useIsPresent()") &&
    hostSource.includes("active={isPresent}") &&
    hostSource.includes('inert={isPresent ? undefined : ""}') &&
    hostSource.includes("aria-hidden={!isPresent}"),
  "exiting page content should become inactive, inert, and hidden from assistive technology",
);
assert.ok(
  hostSource.includes("initial={{ opacity: 0 }}") &&
    hostSource.includes("animate={{ opacity: 1 }}") &&
    hostSource.includes("exit={{ opacity: 0 }}") &&
    hostSource.includes("duration: 0.2"),
  "transient pages should use one 200ms opacity-only transition",
);
assert.ok(
  findForbiddenObjectLiteralProperties(hostSource, hostPath).length === 0,
  "the Motion host should not add movement, scale, or blur",
);

console.log("motion page transition contract ok");
