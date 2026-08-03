import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";

const repoRoot = process.cwd();
const args = process.argv.slice(2);
const mode = readOption("--mode", "baseline");
const scope = readOption("--scope", "all");

const validModes = new Set(["baseline", "force-warn", "verify", "ci"]);
if (!validModes.has(mode)) {
  fail(`unknown --mode '${mode}'`);
}

const cargoManifest = path.join("src-tauri", "Cargo.toml");
const rustSourceRoots = [path.join("src-tauri", "src")];
const rustTestRoots = [path.join("src-tauri", "tests")];

const matrix = [
  {
    id: "default-lib",
    targetKind: "production",
    args: ["check", "--locked", "--manifest-path", cargoManifest, "--lib"],
  },
  {
    id: "all-targets",
    targetKind: "all-targets",
    args: ["check", "--locked", "--manifest-path", cargoManifest, "--all-targets"],
  },
  {
    id: "release-lib",
    targetKind: "release-production",
    args: ["check", "--locked", "--manifest-path", cargoManifest, "--release", "--lib"],
  },
];

main();

function main() {
  const git = readGitState();
  const sourcePolicy = scanSourcePolicy();

  if (mode === "ci") {
    const normal = runCargoDiagnostics(matrix[0]);
    const deadCode = normal.diagnostics.filter((diagnostic) => diagnostic.code === "dead_code");
    const policyFailures = [
      ...sourcePolicy.blanketAllows.map((entry) => `blanket allow(dead_code): ${formatLocation(entry)}`),
      ...sourcePolicy.localAllows.map((entry) => `local allow(dead_code): ${formatLocation(entry)}`),
      ...sourcePolicy.unregisteredExpects.map((entry) => `unregistered expect(dead_code): ${formatLocation(entry)}`),
      ...sourcePolicy.testSupportMentions.map((entry) => `test-support leakage: ${formatLocation(entry)}`),
    ];
    if (deadCode.length > 0 || policyFailures.length > 0) {
      printReport({
        title: "Dead code CI policy",
        git,
        diagnostics: [normal],
        sourcePolicy,
      });
      fail(
        [
          `${deadCode.length} visible dead_code diagnostic group(s)`,
          `${policyFailures.length} source policy violation(s)`,
          ...policyFailures.map((failure) => `- ${failure}`),
        ].join("; "),
      );
    }
    printReport({ title: "Dead code CI policy", git, diagnostics: [normal], sourcePolicy });
    return;
  }

  if (mode === "verify") {
    const normal = runCargoDiagnostics(matrix[0]);
    printReport({
      title: `Dead code verification (${scope})`,
      git,
      diagnostics: [normal],
      sourcePolicy,
    });
    return;
  }

  if (mode === "force-warn") {
    const forced = runCargoDiagnostics(matrix[0], { forceWarnDeadCode: true });
    printReport({
      title: "Dead code force-warn inventory",
      git,
      diagnostics: [forced],
      sourcePolicy,
    });
    return;
  }

  const diagnostics = matrix.map((entry) => runCargoDiagnostics(entry));
  printReport({
    title: "Dead code baseline inventory",
    git,
    diagnostics,
    sourcePolicy,
  });
}

function readOption(name, fallback) {
  const index = args.indexOf(name);
  if (index === -1) return fallback;
  const value = args[index + 1];
  if (!value || value.startsWith("--")) fail(`${name} requires a value`);
  return value;
}

function runCargoDiagnostics(entry, options = {}) {
  const env = { ...process.env };
  if (options.forceWarnDeadCode) {
    env.RUSTFLAGS = appendRustFlag(env.RUSTFLAGS, "--force-warn dead_code");
  }

  const result = spawnCommand("cargo", [...entry.args, "--message-format=json"], {
    cwd: repoRoot,
    encoding: "utf8",
    env,
    maxBuffer: 128 * 1024 * 1024,
  });

  if (result.status !== 0) {
    process.stdout.write(result.stdout ?? "");
    process.stderr.write(result.stderr ?? "");
    fail(`cargo ${entry.args.join(" ")} failed with exit code ${result.status ?? "unknown"}`);
  }

  const diagnostics = [];
  for (const line of String(result.stdout ?? "").split(/\r?\n/)) {
    if (!line.trim()) continue;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      continue;
    }
    if (message.reason !== "compiler-message") continue;
    const diagnostic = message.message;
    const code = diagnostic?.code?.code ?? "";
    if (!code) continue;
    const primarySpan = firstPrimarySpan(diagnostic.spans);
    const file = normalizePath(primarySpan?.file_name ?? "");
    if (!isWorkspaceRustDiagnostic(file)) continue;
    diagnostics.push({
      code,
      level: diagnostic.level,
      message: diagnostic.message,
      target: message.target?.name ?? "",
      targetKinds: message.target?.kind ?? [],
      file,
      line: primarySpan?.line_start ?? 0,
      column: primarySpan?.column_start ?? 0,
      symbol: inferSymbol(primarySpan),
      matrix: entry.id,
      matrixKind: entry.targetKind,
    });
  }

  return {
    id: options.forceWarnDeadCode ? `${entry.id}+force-warn` : entry.id,
    args: entry.args,
    diagnostics,
  };
}

function isWorkspaceRustDiagnostic(file) {
  if (!file) return false;
  const normalized = normalizePath(file);
  if (normalized.startsWith("src/") || normalized.startsWith("tests/")) return true;
  const repo = normalizePath(repoRoot);
  return normalized.startsWith(`${repo}/src-tauri/src/`) || normalized.startsWith(`${repo}/src-tauri/tests/`);
}

function appendRustFlag(current, flag) {
  return current ? `${current} ${flag}` : flag;
}

function firstPrimarySpan(spans = []) {
  return spans.find((span) => span.is_primary) ?? spans[0] ?? null;
}

function inferSymbol(span) {
  if (!span) return "";
  const label = String(span.label ?? "").trim();
  if (label) return label;
  const text = span.text?.[0]?.text ?? "";
  const trimmed = text.trim();
  const patterns = [
    /\b(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:fn|struct|enum|trait|const|static|type|mod)\s+([A-Za-z_][A-Za-z0-9_]*)/,
    /\b([A-Za-z_][A-Za-z0-9_]*)\s*:/,
    /\b([A-Za-z_][A-Za-z0-9_]*)\s*\(/,
    /\b([A-Za-z_][A-Za-z0-9_]*)\b/,
  ];
  for (const pattern of patterns) {
    const match = trimmed.match(pattern);
    if (match) return match[1];
  }
  return trimmed.slice(0, 80);
}

function scanSourcePolicy() {
  const sourceFiles = listRustFiles(rustSourceRoots);
  const testFiles = listRustFiles(rustTestRoots);
  const allFiles = [...sourceFiles, ...testFiles];
  const entries = [];

  for (const file of allFiles) {
    const content = readFileSync(file, "utf8");
    const lines = content.split(/\r?\n/);
    for (const attribute of collectAttributeBlocks(file, lines)) {
      if (/allow\s*\(\s*dead_code\s*\)/.test(attribute.text)) {
        entries.push({ ...attribute, kind: "allow" });
      }
      if (/expect\s*\(\s*dead_code\b/.test(attribute.text)) {
        entries.push({ ...attribute, kind: "expect" });
      }
    }
    lines.forEach((line, index) => {
      if (/cfg\s*\(\s*test\s*\)/.test(line)) {
        entries.push(policyEntry(file, index + 1, line, "cfg-test"));
      }
      if (/test-support/.test(line)) {
        entries.push(policyEntry(file, index + 1, line, "test-support"));
      }
    });
  }

  const allows = entries.filter((entry) => entry.kind === "allow");
  const expects = entries.filter((entry) => entry.kind === "expect");
  return {
    blanketAllows: allows.filter((entry) => /#!\s*\[/.test(entry.text) || /cfg_attr\s*\(\s*not\s*\(\s*test\s*\)/.test(entry.text)),
    localAllows: allows.filter((entry) => !(/#!\s*\[/.test(entry.text) || /cfg_attr\s*\(\s*not\s*\(\s*test\s*\)/.test(entry.text))),
    expects,
    unregisteredExpects: expects.filter((entry) => !/contract=.+owner=.+remove_when=/.test(entry.text)),
    cfgTests: entries.filter((entry) => entry.kind === "cfg-test"),
    testSupportMentions: entries.filter((entry) => entry.kind === "test-support"),
  };
}

function collectAttributeBlocks(file, lines) {
  const attributes = [];
  for (let index = 0; index < lines.length; index += 1) {
    const trimmed = lines[index].trim();
    if (!trimmed.startsWith("#[") && !trimmed.startsWith("#![")) continue;

    const startLine = index + 1;
    const block = [];
    let depth = 0;
    do {
      const line = lines[index] ?? "";
      block.push(line);
      depth += countChar(line, "[") - countChar(line, "]");
      if (depth <= 0) break;
      index += 1;
    } while (index < lines.length);

    attributes.push(
      policyEntry(file, startLine, block.join(" ").replace(/\s+/g, " "), "attribute"),
    );
  }
  return attributes;
}

function countChar(value, character) {
  return [...String(value)].filter((candidate) => candidate === character).length;
}

function listRustFiles(roots) {
  const files = [];
  for (const root of roots) {
    const absoluteRoot = path.join(repoRoot, root);
    if (!existsSync(absoluteRoot)) continue;
    walk(absoluteRoot, files);
  }
  return files;
}

function walk(directory, files) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      walk(fullPath, files);
    } else if (entry.isFile() && entry.name.endsWith(".rs")) {
      files.push(fullPath);
    }
  }
}

function policyEntry(file, line, text, kind) {
  return {
    file: normalizePath(path.relative(repoRoot, file)),
    line,
    kind,
    text: text.trim(),
  };
}

function readGitState() {
  return {
    head: runText("git", ["rev-parse", "--short", "HEAD"]),
    branch: runText("git", ["branch", "--show-current"]),
    status: runText("git", ["status", "--short", "--branch"]).split(/\r?\n/).filter(Boolean),
  };
}

function runText(command, commandArgs) {
  const result = spawnCommand(command, commandArgs, {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) return "";
  return String(result.stdout ?? "").trim();
}

function printReport({ title, git, diagnostics, sourcePolicy }) {
  const deadCodeByMatrix = diagnostics.map((entry) => ({
    id: entry.id,
    totalDiagnostics: entry.diagnostics.length,
    deadCode: entry.diagnostics.filter((diagnostic) => diagnostic.code === "dead_code"),
  }));
  const allDeadCode = diagnostics.flatMap((entry) =>
    entry.diagnostics
      .filter((diagnostic) => diagnostic.code === "dead_code")
      .map((diagnostic) => ({ ...diagnostic, sourceMatrix: entry.id })),
  );
  const uniqueDeadCode = uniqueBy(
    allDeadCode,
    (diagnostic) => `${diagnostic.file}:${diagnostic.line}:${diagnostic.column}:${diagnostic.code}:${diagnostic.symbol}`,
  ).map((diagnostic) => ({
    ...diagnostic,
    matrices: allDeadCode
      .filter(
        (other) =>
          other.file === diagnostic.file &&
          other.line === diagnostic.line &&
          other.column === diagnostic.column &&
          other.symbol === diagnostic.symbol,
      )
      .map((other) => other.sourceMatrix),
  }));

  console.log(`# ${title}`);
  console.log("");
  console.log(`- head: ${git.head}`);
  console.log(`- branch: ${git.branch}`);
  console.log(`- scope: ${scope}`);
  console.log("");
  console.log("## Cargo matrix");
  console.log("");
  console.log("| matrix | total diagnostics | dead_code groups |");
  console.log("|---|---:|---:|");
  for (const entry of deadCodeByMatrix) {
    console.log(`| ${entry.id} | ${entry.totalDiagnostics} | ${entry.deadCode.length} |`);
  }
  console.log("");
  console.log(`Unique dead_code identities: ${uniqueDeadCode.length}`);
  console.log("");
  printDiagnostics(uniqueDeadCode);
  console.log("");
  console.log("## Source policy");
  console.log("");
  console.log(`- blanket allow(dead_code): ${sourcePolicy.blanketAllows.length}`);
  console.log(`- local allow(dead_code): ${sourcePolicy.localAllows.length}`);
  console.log(`- expect(dead_code): ${sourcePolicy.expects.length}`);
  console.log(`- cfg(test): ${sourcePolicy.cfgTests.length}`);
  console.log(`- test-support mentions: ${sourcePolicy.testSupportMentions.length}`);
  console.log("");
  printPolicyList("blanket allow(dead_code)", sourcePolicy.blanketAllows);
  printPolicyList("local allow(dead_code)", sourcePolicy.localAllows);
  printPolicyList("expect(dead_code)", sourcePolicy.expects);
}

function printDiagnostics(diagnostics) {
  if (diagnostics.length === 0) {
    console.log("No dead_code diagnostics.");
    return;
  }
  console.log("| file | line | symbol | matrices |");
  console.log("|---|---:|---|---|");
  for (const diagnostic of diagnostics) {
    const matrices = [...new Set(diagnostic.matrices ?? [])].join(", ");
    console.log(
      `| ${diagnostic.file} | ${diagnostic.line} | ${escapeCell(diagnostic.symbol)} | ${escapeCell(matrices)} |`,
    );
  }
}

function printPolicyList(title, entries) {
  if (entries.length === 0) return;
  console.log(`### ${title}`);
  console.log("");
  console.log("| file | line | text |");
  console.log("|---|---:|---|");
  for (const entry of entries) {
    console.log(`| ${entry.file} | ${entry.line} | ${escapeCell(entry.text)} |`);
  }
  console.log("");
}

function formatLocation(entry) {
  return `${entry.file}:${entry.line}`;
}

function normalizePath(value) {
  return String(value).replaceAll("\\", "/");
}

function spawnCommand(command, commandArgs, options) {
  if (process.platform !== "win32") {
    return spawnSync(command, commandArgs, options);
  }
  return spawnSync("cmd.exe", ["/d", "/s", "/c", command, ...commandArgs], options);
}

function escapeCell(value) {
  return String(value).replaceAll("|", "\\|").replace(/\s+/g, " ").trim();
}

function uniqueBy(items, keyFn) {
  const seen = new Set();
  const unique = [];
  for (const item of items) {
    const key = keyFn(item);
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(item);
  }
  return unique;
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
