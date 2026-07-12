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

for (const sourcePath of sourceFiles) {
  const source = await readFile(sourcePath, "utf8");
  if (sourceReferencesFramerMotion(source, sourcePath)) {
    motionImporters.push(path.normalize(sourcePath));
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
    pageActivitySource.includes("const active = useInteractionActivity();") &&
    !pageActivitySource.includes("createContext("),
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
  motionImporters,
  [hostPath],
  "TransientPageHost should be the only source module importing framer-motion",
);
assert.ok(
  hostSource.includes('<MotionConfig reducedMotion="user">') &&
    hostSource.includes('<AnimatePresence initial={false} mode="wait">'),
  "the host should centralize reduced-motion and wait-mode presence behavior",
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
