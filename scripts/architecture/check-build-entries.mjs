import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import { assert, readJson, repoRoot, runMain } from "./lib.mjs";

function htmlEntry(file) {
  const html = fs.readFileSync(path.join(repoRoot, file), "utf8");
  const matches = [...html.matchAll(/<script\b[^>]*\btype=["']module["'][^>]*\bsrc=["']([^"']+)["']/gi)];
  assert(matches.length === 1, `${file} must contain exactly one module entry`);
  return matches[0][1].replace(/^\//, "");
}

function resolvedImports(entry) {
  const configPath = ts.findConfigFile(repoRoot, ts.sys.fileExists, "tsconfig.json");
  assert(configPath, "tsconfig.json is required");
  const loaded = ts.readConfigFile(configPath, ts.sys.readFile);
  assert(!loaded.error, "tsconfig.json is invalid");
  const parsed = ts.parseJsonConfigFileContent(loaded.config, ts.sys, repoRoot);
  const program = ts.createProgram({ rootNames: parsed.fileNames, options: parsed.options });
  const byName = new Map(program.getSourceFiles().map((source) => [path.resolve(source.fileName), source]));
  const visited = new Set();
  const queue = [path.resolve(repoRoot, entry)];
  while (queue.length) {
    const current = queue.pop();
    if (visited.has(current)) continue;
    visited.add(current);
    const source = byName.get(current);
    assert(source, `production entry graph cannot resolve ${path.relative(repoRoot, current)}`);
    function visit(node) {
      let specifier;
      if ((ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) && node.moduleSpecifier && ts.isStringLiteralLike(node.moduleSpecifier)) specifier = node.moduleSpecifier.text;
      if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
        assert(node.arguments.length === 1 && ts.isStringLiteralLike(node.arguments[0]), `production dynamic import in ${source.fileName} must use one string literal`);
        specifier = node.arguments[0].text;
      }
      if (specifier) {
        const resolved = ts.resolveModuleName(specifier, source.fileName, parsed.options, ts.sys).resolvedModule;
        if (resolved && path.resolve(resolved.resolvedFileName).startsWith(repoRoot)) queue.push(path.resolve(resolved.resolvedFileName));
      }
      ts.forEachChild(node, visit);
    }
    visit(source);
  }
  return visited;
}

runMain(() => {
  const config = readJson("src-tauri/tauri.conf.json", "production Tauri config");
  assert(config.build?.beforeBuildCommand === "pnpm build", "production Tauri build must call the production build script");
  assert(config.build?.frontendDist === "../dist", "production Tauri frontendDist must be ../dist");
  const entry = htmlEntry("index.html");
  assert(entry === "src/main.tsx", `production HTML entry must be src/main.tsx, got ${entry}`);
  const graph = resolvedImports(entry);
  const demoReachable = [...graph].filter((file) => /(?:^|[\\/])demo(?:[.\\/]|$)|DemoBackend/i.test(file));
  assert(demoReachable.length === 0, `production entry reaches demo assets: ${demoReachable.map((file) => path.relative(repoRoot, file)).join(", ")}`);
  console.log(`Build entry gate passed (${graph.size} production modules)`);
});
