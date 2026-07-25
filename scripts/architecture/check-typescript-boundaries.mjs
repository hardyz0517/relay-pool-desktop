import path from "node:path";
import ts from "typescript";
import {
  assert,
  assertOwnedExpiry,
  authoritativeStage,
  normalizePath,
  readRequiredManifest,
  relativeToRoot,
  repoRoot,
  runMain,
} from "./lib.mjs";

const SOURCE_EXTENSIONS = new Set([".ts", ".tsx", ".mts", ".cts"]);
const STATIC_ASSET_SPECIFIER = /\.(?:css|scss|sass|less|svg|png|jpe?g|gif|webp|ico|woff2?)(?:\?.*)?$/i;

function loadProgram(configPath) {
  const absoluteConfig = path.resolve(repoRoot, configPath);
  const config = ts.readConfigFile(absoluteConfig, ts.sys.readFile);
  assert(!config.error, ts.flattenDiagnosticMessageText(config.error?.messageText ?? "", "\n"));
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, path.dirname(absoluteConfig));
  assert(parsed.errors.length === 0, parsed.errors.map((error) => ts.flattenDiagnosticMessageText(error.messageText, "\n")).join("\n"));
  const program = ts.createProgram({ rootNames: parsed.fileNames, options: parsed.options });
  return { program, options: parsed.options };
}

function ownerOf(file) {
  const normalized = normalizePath(file);
  const match = normalized.match(/(?:^|\/)src\/features\/([^/]+)(?:\/|$)/);
  return match ? `feature:${match[1]}` : normalized.includes("/src/") || normalized.startsWith("src/") ? "shared" : "external";
}

function isFeaturePublicEntry(file, owner) {
  if (!owner.startsWith("feature:")) return false;
  const feature = owner.slice("feature:".length);
  const normalized = normalizePath(file);
  return ["ts", "tsx"].some((extension) =>
    normalized === `src/features/${feature}/index.${extension}` ||
    normalized.endsWith(`/src/features/${feature}/index.${extension}`),
  );
}

function isCompositionRoot(file) {
  const normalized = normalizePath(file);
  return normalized === "src/main.tsx"
    || normalized === "src/app/App.tsx"
    || normalized === "src/app/shellPageRegistry.tsx";
}

function collectSpecifiers(sourceFile) {
  const result = [];
  function visit(node) {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) {
      if (node.moduleSpecifier && ts.isStringLiteralLike(node.moduleSpecifier)) {
        result.push({
          specifier: node.moduleSpecifier.text,
          kind: ts.isExportDeclaration(node) ? "re-export" : node.importClause?.isTypeOnly ? "type" : "static",
        });
      }
    } else if (
      ts.isCallExpression(node) &&
      node.expression.kind === ts.SyntaxKind.ImportKeyword
    ) {
      assert(node.arguments.length === 1 && ts.isStringLiteralLike(node.arguments[0]), `dynamic import in ${sourceFile.fileName} must use one string literal`);
      result.push({ specifier: node.arguments[0].text, kind: "dynamic" });
    }
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
  return result;
}

function edgeIdentity(edge) {
  return `${edge.from} -> ${edge.to} [${edge.kind}]`;
}

function analyze(configPath) {
  const { program, options } = loadProgram(configPath);
  const edges = [];
  for (const sourceFile of program.getSourceFiles()) {
    if (sourceFile.isDeclarationFile || !SOURCE_EXTENSIONS.has(path.extname(sourceFile.fileName))) continue;
    if (!path.resolve(sourceFile.fileName).startsWith(repoRoot)) continue;
    const from = relativeToRoot(sourceFile.fileName);
    if (/\.(?:test|spec)\.[cm]?[jt]sx?$/.test(from) || from.includes("/test/")) continue;
    for (const imported of collectSpecifiers(sourceFile)) {
      if (STATIC_ASSET_SPECIFIER.test(imported.specifier) || imported.specifier.startsWith("node:")) continue;
      const resolved = ts.resolveModuleName(imported.specifier, sourceFile.fileName, options, ts.sys).resolvedModule;
      assert(resolved, `cannot resolve '${imported.specifier}' imported by ${from}`);
      if (!path.resolve(resolved.resolvedFileName).startsWith(repoRoot)) continue;
      const to = relativeToRoot(resolved.resolvedFileName).replace(/\.d\.ts$/, ".ts");
      edges.push({ from, to, kind: imported.kind, fromOwner: ownerOf(from), toOwner: ownerOf(to) });
    }
  }
  return edges.sort((a, b) => edgeIdentity(a).localeCompare(edgeIdentity(b)));
}

function normalizeAllowlist(manifest) {
  const currentStage = authoritativeStage(manifest, "boundary manifest");
  const normalize = (entry) => {
    if (typeof entry === "string") return [normalizePath(entry)];
    if (!entry || typeof entry !== "object") return [];
    if (entry.ecosystem && entry.ecosystem !== "typescript") return [];
    if (Array.isArray(entry.identities)) return entry.identities.map(normalizePath);
    if (entry.identity) return [normalizePath(entry.identity)];
    if (entry.from && entry.to) return [`${normalizePath(entry.from)} -> ${normalizePath(entry.to)} [${entry.kind ?? "static"}]`];
    return [];
  };
  const allowed = new Set((manifest.allowed_edges ?? []).flatMap(normalize));
  const temporary = new Set();
  for (const [index, entry] of (manifest.temporary_edges ?? []).entries()) {
    if (entry?.ecosystem && entry.ecosystem !== "typescript") continue;
    assertOwnedExpiry(entry, `temporary_edges[${index}]`, currentStage);
    assert(typeof entry.reason === "string" && entry.reason.trim(), `temporary_edges[${index}].reason is required`);
    for (const identity of normalize(entry)) temporary.add(identity);
  }
  return { allowed, temporary, combined: new Set([...allowed, ...temporary]) };
}

function forbiddenViolations(edges, manifest) {
  const violations = [];
  for (const entry of manifest.forbidden_edges ?? []) {
    if (entry?.ecosystem && entry.ecosystem !== "typescript") continue;
    if (typeof entry === "string" || entry?.identity) {
      const identity = normalizePath(typeof entry === "string" ? entry : entry.identity);
      if (edges.some((edge) => edgeIdentity(edge) === identity)) violations.push(`manifest-forbidden edge exists: ${identity}`);
      continue;
    }
    if (entry?.from_owner && entry?.to_owner) {
      for (const edge of edges.filter((edge) => edge.fromOwner === entry.from_owner && edge.toOwner === entry.to_owner)) {
        violations.push(`manifest-forbidden owner edge exists: ${edgeIdentity(edge)}`);
      }
    }
  }
  return violations;
}

function detectOwnerCycles(edges) {
  const adjacency = new Map();
  for (const edge of edges) {
    if (!edge.fromOwner.startsWith("feature:") || !edge.toOwner.startsWith("feature:") || edge.fromOwner === edge.toOwner) continue;
    if (!adjacency.has(edge.fromOwner)) adjacency.set(edge.fromOwner, new Set());
    adjacency.get(edge.fromOwner).add(edge.toOwner);
  }
  const cycles = [];
  const visiting = new Set();
  const visited = new Set();
  function walk(node, stack) {
    if (visiting.has(node)) {
      cycles.push([...stack.slice(stack.indexOf(node)), node].join(" -> "));
      return;
    }
    if (visited.has(node)) return;
    visiting.add(node);
    for (const next of adjacency.get(node) ?? []) walk(next, [...stack, next]);
    visiting.delete(node);
    visited.add(node);
  }
  for (const node of adjacency.keys()) walk(node, [node]);
  return [...new Set(cycles)];
}

function transitiveBoundaryIdentities(edges) {
  const adjacency = new Map();
  for (const edge of edges) {
    if (!adjacency.has(edge.from)) adjacency.set(edge.from, []);
    adjacency.get(edge.from).push(edge);
  }
  const identities = [];
  for (const start of adjacency.keys()) {
    const startOwner = ownerOf(start);
    if (!startOwner.startsWith("feature:")) continue;
    const queue = (adjacency.get(start) ?? []).map((edge) => ({ edge, path: [edge] }));
    const visited = new Set();
    while (queue.length) {
      const { edge, path: route } = queue.shift();
      const state = `${edge.to}|${route[0].to}`;
      if (visited.has(state)) continue;
      visited.add(state);
      const targetOwner = ownerOf(edge.to);
      if (targetOwner.startsWith("feature:") && targetOwner !== startOwner && route.length > 1) {
        const enteredPublicIndex = route.length === 1 && isFeaturePublicEntry(edge.to, targetOwner);
        const identity = `transitive:${start} => ${edge.to} [via ${route[0].to}]`;
        if (!enteredPublicIndex) identities.push(identity);
        continue;
      }
      if (isCompositionRoot(edge.to)) continue;
      for (const next of adjacency.get(edge.to) ?? []) queue.push({ edge: next, path: [...route, next] });
    }
  }
  return [...new Set(identities)];
}

export function checkProject(configPath, manifest) {
  const edges = analyze(configPath);
  const allowlist = normalizeAllowlist(manifest);
  const violations = forbiddenViolations(edges, manifest);
  for (const edge of edges) {
    const identity = edgeIdentity(edge);
    if (edge.fromOwner.startsWith("feature:") && isCompositionRoot(edge.to) && !allowlist.combined.has(identity)) {
      violations.push(`feature imports composition root: ${identity}`);
      continue;
    }
    if (edge.fromOwner === "shared" && edge.toOwner.startsWith("feature:") && !isCompositionRoot(edge.from) && !allowlist.combined.has(identity)) {
      violations.push(`shared layer imports feature implementation: ${identity}`);
      continue;
    }
    if (!edge.fromOwner.startsWith("feature:") || !edge.toOwner.startsWith("feature:") || edge.fromOwner === edge.toOwner) continue;
    const isPublicEntry = isFeaturePublicEntry(edge.to, edge.toOwner);
    if (!isPublicEntry && !allowlist.combined.has(identity)) violations.push(`cross-feature descendant import: ${identity}`);
  }
  for (const cycle of detectOwnerCycles(edges)) {
    if (!allowlist.combined.has(`cycle:${cycle}`)) violations.push(`cross-feature cycle: ${cycle}`);
  }
  const transitiveIdentities = transitiveBoundaryIdentities(edges);
  violations.push(...transitiveIdentities
    .filter((identity) => !allowlist.combined.has(identity))
    .map((identity) => `transitive cross-feature descendant: ${identity}`));
  const actualIdentities = new Set([...edges.map(edgeIdentity), ...transitiveIdentities]);
  for (const identity of allowlist.temporary) {
    if (identity.startsWith("cycle:")) continue;
    if (!actualIdentities.has(identity)) violations.push(`stale temporary TypeScript edge: ${identity}`);
  }
  assert(violations.length === 0, violations.join("\n"));
  return edges;
}

function fixtureManifest(allowedEdges = []) {
  return { current_stage: 0, allowed_edges: allowedEdges, temporary_edges: [], forbidden_edges: [] };
}

function runFixtures() {
  const base = "scripts/architecture/fixtures/typescript";
  const passEdges = checkProject(`${base}/pass/tsconfig.json`, fixtureManifest());
  assert(passEdges.some((edge) => edge.kind === "type"), "type-only import fixture was not parsed");
  const identityEdges = analyze(`${base}/identity/tsconfig.json`);
  assert(new Set(identityEdges.filter((edge) => edge.to.endsWith("/local.ts")).map((edge) => edge.to)).size === 2, "same-name symbols must retain declaring-file identity");
  assert(analyze(`${base}/red-fanout/tsconfig.json`).some((edge) => edge.kind === "dynamic"), "dynamic import fixture was not parsed");
  for (const fixture of ["red-cross-feature", "red-cycle", "red-barrel", "red-fanout"]) {
    let failed = false;
    try {
      checkProject(`${base}/${fixture}/tsconfig.json`, fixtureManifest());
    } catch {
      failed = true;
    }
    assert(failed, `${fixture} must be rejected`);
  }
  let staleRejected = false;
  try {
    checkProject(`${base}/pass/tsconfig.json`, {
      current_stage: 0,
      allowed_edges: [],
      forbidden_edges: [],
      temporary_edges: [{
        ecosystem: "typescript",
        identity: "src/features/alpha/missing.ts -> src/features/beta/index.ts [static]",
        owner: "fixture",
        reason: "stale fixture",
        expiry_stage: 2,
      }],
    });
  } catch {
    staleRejected = true;
  }
  assert(staleRejected, "stale TypeScript temporary edge must be rejected");
  let expiredRejected = false;
  try {
    checkProject(`${base}/pass/tsconfig.json`, {
      current_stage: 1,
      allowed_edges: [],
      forbidden_edges: [],
      temporary_edges: [{
        ecosystem: "typescript",
        identity: edgeIdentity(passEdges[0]),
        owner: "fixture",
        reason: "expired fixture",
        expiry_stage: 1,
      }],
    });
  } catch {
    expiredRejected = true;
  }
  assert(expiredRejected, "expired TypeScript temporary edge must be rejected");
}

runMain(() => {
  if (process.argv.includes("--fixtures")) {
    runFixtures();
    console.log("TypeScript architecture fixtures passed");
    return;
  }
  const manifest = readRequiredManifest("docs/superpowers/audits/architecture-scale-boundary-manifest.json", [
    "current_stage",
    "allowed_edges",
    "forbidden_edges",
    "temporary_edges",
    "fan_in_baseline",
    "fan_out_baseline",
  ]);
  const edges = checkProject("tsconfig.json", manifest);
  assert(Object.keys(manifest.fan_in_baseline).length > 0, "fan_in_baseline must not be empty");
  assert(Object.keys(manifest.fan_out_baseline).length > 0, "fan_out_baseline must not be empty");
  console.log(`TypeScript architecture gate passed (${edges.length} resolved edges)`);
});
