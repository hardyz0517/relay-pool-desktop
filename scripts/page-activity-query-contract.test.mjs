import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

function parseTsxSource(source, fileName) {
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

function importedLocalNames(sourceFile, moduleName, importedName) {
  const names = new Set();
  for (const node of findNodes(sourceFile, ts.isImportDeclaration)) {
    if (
      !ts.isStringLiteralLike(node.moduleSpecifier) ||
      node.moduleSpecifier.text !== moduleName ||
      !node.importClause?.namedBindings ||
      !ts.isNamedImports(node.importClause.namedBindings)
    ) {
      continue;
    }
    for (const element of node.importClause.namedBindings.elements) {
      if ((element.propertyName ?? element.name).text === importedName) {
        names.add(element.name.text);
      }
    }
  }
  return names;
}

function getObjectLiteralPropertyInitializer(objectLiteral, propertyName) {
  const property = objectLiteral.properties.find(
    (entry) =>
      ts.isPropertyAssignment(entry) &&
      ((ts.isIdentifier(entry.name) && entry.name.text === propertyName) ||
        (ts.isStringLiteralLike(entry.name) && entry.name.text === propertyName)),
  );
  return property?.initializer;
}

const visibilitySourcePath = "src/app/navigation/PageVisibility.tsx";
const querySourcePath = "src/lib/query/useActivityQuery.ts";
const visibilitySource = await readFile(visibilitySourcePath, "utf8");
const querySource = await readFile(querySourcePath, "utf8");
const visibilityFile = parseTsxSource(visibilitySource, visibilitySourcePath);
const queryFile = parseTsxSource(querySource, querySourcePath);

assert.equal(
  await readFile("src/components/shell/PageActivity.tsx", "utf8").then(
    () => true,
    () => false,
  ),
  false,
  "legacy PageActivity compatibility module should be deleted",
);

const interactionProviderNames = importedLocalNames(
  visibilityFile,
  "@/components/ui/InteractionActivity",
  "InteractionActivityProvider",
);
assert.ok(
  findNodes(
    visibilityFile,
    (node) =>
      (ts.isJsxOpeningElement(node) || ts.isJsxSelfClosingElement(node)) &&
      ts.isIdentifier(node.tagName) &&
      interactionProviderNames.has(node.tagName.text) &&
      node.attributes.properties.some(
        (attribute) =>
          ts.isJsxAttribute(attribute) &&
          ts.isIdentifier(attribute.name) &&
          attribute.name.text === "active" &&
          attribute.initializer &&
          ts.isJsxExpression(attribute.initializer) &&
          ts.isPropertyAccessExpression(attribute.initializer.expression) &&
          ts.isIdentifier(attribute.initializer.expression.expression) &&
          attribute.initializer.expression.expression.text === "value" &&
          attribute.initializer.expression.name.text === "interactive",
      ),
  ).length === 1,
  "PageVisibilityProvider should drive shared interaction activity from canonical visibility",
);

const pageQueryNames = importedLocalNames(
  queryFile,
  "@/app/navigation/PageVisibility",
  "usePageQueryEnabled",
);
assert.ok(
  findNodes(
    queryFile,
    (node) =>
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      pageQueryNames.has(node.expression.text),
  ).length === 1,
  "useActivityQuery should read canonical page query visibility",
);

const useQueryNames = importedLocalNames(queryFile, "@tanstack/react-query", "useQuery");
const useQueryCalls = findNodes(
  queryFile,
  (node) =>
    ts.isCallExpression(node) &&
    ts.isIdentifier(node.expression) &&
    useQueryNames.has(node.expression.text) &&
    node.arguments.length === 1 &&
    ts.isObjectLiteralExpression(node.arguments[0]),
);
assert.equal(useQueryCalls.length, 1, "useActivityQuery should wrap one useQuery call");

const useQueryOptions = useQueryCalls[0].arguments[0];
const enabledInitializer = getObjectLiteralPropertyInitializer(useQueryOptions, "enabled");
const subscribedInitializer = getObjectLiteralPropertyInitializer(useQueryOptions, "subscribed");
assert.ok(
  enabledInitializer &&
    ts.isIdentifier(enabledInitializer) &&
    enabledInitializer.text === "queryEnabled",
  "useActivityQuery should gate query execution with queryEnabled",
);
assert.ok(
  subscribedInitializer &&
    ts.isIdentifier(subscribedInitializer) &&
    subscribedInitializer.text === "active",
  "useActivityQuery should disable query subscription while hidden",
);
assert.ok(
  querySource.includes("recordHiddenPageQueryStart()"),
  "hidden query attempts should keep the navigation performance counter",
);
assert.ok(!querySource.includes("setInterval"), "activity query must not own polling timers");

console.log("page activity query contract passed");
