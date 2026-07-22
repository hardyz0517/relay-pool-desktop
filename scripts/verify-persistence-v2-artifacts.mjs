import { createHash } from "node:crypto";
import { mkdtemp, readFile, readdir, rm, stat } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { basename, isAbsolute, join, relative, resolve, sep } from "node:path";
import { pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";

const SCRIPT_DIR = resolve(import.meta.dirname);
const DEFAULT_REPO_ROOT = resolve(SCRIPT_DIR, "..");
const DEFAULT_POLICY = join(SCRIPT_DIR, "persistence-v2-artifact-policy.json");

const forbiddenArtifactName = /(?:\.(?:db|sqlite|sqlite3)(?:[.-].*)?|\.(?:bak|backup|log)(?:\.\d+)?)$/i;
const generatedDirectory = /^(?:target|src-tauri\/target|dist|dist-ssr|logs|backups?|local-data|data)(?:\/|$)/i;

function normalizePath(value) {
  return value.replaceAll("\\", "/").replace(/^\.\//, "");
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: options.encoding ?? "utf8",
    cwd: options.cwd,
    input: options.input,
    maxBuffer: 256 * 1024 * 1024,
    shell: false,
  });
  if (result.status !== 0) {
    throw new Error(`${command} failed with exit code ${result.status ?? "unknown"}`);
  }
  return result.stdout;
}

function parseArguments(argv) {
  const options = {
    artifacts: [],
    canaries: [],
    policyPath: DEFAULT_POLICY,
    repoRoot: DEFAULT_REPO_ROOT,
    sqlite: [],
    tracked: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const next = () => {
      const value = argv[++index];
      if (!value) throw new Error(`${argument} requires a value`);
      return value;
    };
    if (argument === "--artifact") options.artifacts.push(next());
    else if (argument === "--canary") options.canaries.push(next());
    else if (argument === "--policy") options.policyPath = next();
    else if (argument === "--repo-root") options.repoRoot = next();
    else if (argument === "--sqlite") options.sqlite.push(next());
    else if (argument === "--tracked") options.tracked = true;
    else throw new Error(`unknown argument: ${argument}`);
  }
  if (!options.tracked && options.artifacts.length === 0 && options.sqlite.length === 0) {
    throw new Error("select at least one scan target: --tracked, --sqlite, or --artifact");
  }
  options.repoRoot = resolve(options.repoRoot);
  options.policyPath = resolve(options.policyPath);
  options.artifacts = options.artifacts.map((value) => resolve(options.repoRoot, value));
  options.sqlite = options.sqlite.map((value) => resolve(options.repoRoot, value));
  return options;
}

function sha256(buffer) {
  return createHash("sha256").update(buffer).digest("hex");
}

function pathNeedles(repoRoot) {
  const values = new Set([resolve(repoRoot), resolve(homedir())]);
  return [...values].flatMap((value) => [value, value.replaceAll("\\", "/")]);
}

function scanBuffer(buffer, label, { artifact, canaries, repoRoot }) {
  const findings = [];
  const text = buffer.toString("utf8");
  const pathText = text.replaceAll("\\\\", "\\");
  for (const canary of canaries) {
    if (canary && buffer.includes(Buffer.from(canary))) {
      findings.push(`${label}: seeded secret canary is present`);
    }
  }

  const highConfidencePatterns = [
    ["private key", /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/],
    ["OpenAI-style key", /\bsk-(?:(?:proj|svcacct)-)?[A-Za-z0-9]{32,}\b/],
    ["GitHub token", /\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})\b/],
    ["AWS access key", /\bAKIA[0-9A-Z]{16}\b/],
    ["bearer token", /\bbearer\s+[A-Za-z0-9._~+/]{32,}={0,2}/i],
  ];
  for (const [kind, pattern] of highConfidencePatterns) {
    if (pattern.test(text)) findings.push(`${label}: ${kind} is present`);
  }

  const needles = artifact ? [] : pathNeedles(repoRoot);
  for (const needle of needles) {
    if (needle && text.includes(needle)) {
      findings.push(`${label}: local absolute path is present`);
      break;
    }
  }
  if (artifact && /[A-Za-z]:[\\/]Users[\\/][^\\/\0\r\n]+[\\/]/i.test(pathText)) {
    findings.push(`${label}: absolute Windows user path is present`);
  }
  if (artifact && /\/home\/[^/\0\r\n]+\//.test(pathText)) {
    findings.push(`${label}: absolute Unix user path is present`);
  }
  return findings;
}

function parseIndexEntries(repoRoot) {
  const output = run("git", ["-C", repoRoot, "ls-files", "--stage", "-z"], {
    encoding: "buffer",
  });
  return output
    .toString("utf8")
    .split("\0")
    .filter(Boolean)
    .map((entry) => {
      const match = /^(\d+) ([0-9a-f]+) (\d)\t([\s\S]+)$/.exec(entry);
      if (!match) throw new Error(`unable to parse Git index entry: ${entry}`);
      return { mode: match[1], oid: match[2], stage: Number(match[3]), path: normalizePath(match[4]) };
    });
}

async function inspectSqlite(databasePath, allowedSensitiveValues, canaries) {
  const helper = join(SCRIPT_DIR, "scan-sqlite-artifact.py");
  const input = JSON.stringify({ allowedSensitiveValues, canaries });
  const result = spawnSync("python", [helper, databasePath], {
    encoding: "utf8",
    input,
    maxBuffer: 256 * 1024 * 1024,
    shell: false,
  });
  let payload;
  try {
    payload = JSON.parse(result.stdout);
  } catch {
    throw new Error("SQLite scanner failed without structured output");
  }
  if (result.status !== 0 || !payload.ok) return payload.findings ?? ["SQLite scanner failed"];
  return [];
}

async function scanTrackedIndex(repoRoot, policy, canaries) {
  const findings = [];
  const entries = parseIndexEntries(repoRoot);
  const conflicts = entries.filter((entry) => entry.stage !== 0);
  if (conflicts.length > 0) {
    findings.push("Git index contains unresolved merge stages");
    return findings;
  }
  const unsupportedEntries = entries.filter((entry) => !entry.mode.startsWith("100"));
  if (unsupportedEntries.length > 0) {
    findings.push(
      ...unsupportedEntries.map(
        (entry) => `${entry.path}: symbolic links, submodules, and special index entries are not allowed`,
      ),
    );
    return findings;
  }
  const byPath = new Map(entries.map((entry) => [entry.path, entry]));
  const fixturePolicy = new Map(
    policy.trackedDatabaseFixtures.map((entry) => [normalizePath(entry.path), entry]),
  );
  const indexRoot = await mkdtemp(join(tmpdir(), "relay-pool-index-scan-"));
  try {
    run("git", ["-C", repoRoot, "checkout-index", "--all", `--prefix=${indexRoot}${sep}`]);
    for (const entry of entries) {
      const fixture = fixturePolicy.get(entry.path);
      if (generatedDirectory.test(entry.path)) {
        findings.push(`${entry.path}: generated/local data directory is tracked`);
      }
      if (forbiddenArtifactName.test(basename(entry.path)) && !fixture) {
        findings.push(`${entry.path}: database, journal, backup, or log artifact is tracked`);
        continue;
      }

      const indexedPath = join(indexRoot, ...entry.path.split("/"));
      const buffer = await readFile(indexedPath);
      if (!fixture) {
        findings.push(...scanBuffer(buffer, entry.path, { artifact: false, canaries, repoRoot }));
        continue;
      }

      const manifestPath = normalizePath(fixture.manifest);
      const manifestEntry = byPath.get(manifestPath);
      if (!manifestEntry) {
        findings.push(`${entry.path}: allowlisted fixture manifest is not tracked`);
        continue;
      }
      let manifest;
      try {
        const indexedManifest = join(indexRoot, ...manifestPath.split("/"));
        const manifestText = (await readFile(indexedManifest, "utf8")).replace(/^\uFEFF/, "");
        manifest = JSON.parse(manifestText);
      } catch (error) {
        findings.push(`${manifestPath}: invalid fixture manifest: ${error.message}`);
        continue;
      }
      const actualHash = sha256(buffer);
      if (manifest.fixture_sha256 !== actualHash) {
        findings.push(`${entry.path}: SHA-256 does not match ${manifestPath}`);
        continue;
      }

      const sqliteFindings = await inspectSqlite(
        indexedPath,
        fixture.allowedSensitiveValues ?? [],
        canaries,
      );
      findings.push(...sqliteFindings.map((finding) => `${entry.path}: ${finding}`));
    }
  } finally {
    await rm(indexRoot, { force: true, recursive: true });
  }
  return findings;
}

async function scanArtifactPath(root, options) {
  const findings = [];
  let rootStats;
  try {
    rootStats = await stat(root);
  } catch {
    throw new Error(`artifact target does not exist: ${basename(root)}`);
  }
  const files = [];
  if (rootStats.isFile()) files.push(root);
  else if (rootStats.isDirectory()) {
    const pending = [root];
    while (pending.length > 0) {
      const directory = pending.pop();
      for (const entry of await readdir(directory, { withFileTypes: true })) {
        const path = join(directory, entry.name);
        if (entry.isDirectory()) pending.push(path);
        else if (entry.isFile()) files.push(path);
        else {
          const label = normalizePath(relative(root, path)) || entry.name;
          findings.push(`${label}: symbolic links and special files are not allowed in release artifacts`);
        }
      }
    }
  } else {
    findings.push(`${root}: artifact target is not a regular file or directory`);
  }

  for (const file of files) {
    const label = normalizePath(relative(root, file)) || basename(file);
    if (forbiddenArtifactName.test(basename(file))) {
      findings.push(`${label}: database, journal, backup, or log artifact is bundled`);
    }
    const buffer = await readFile(file);
    findings.push(...scanBuffer(buffer, label, { ...options, artifact: true }));
  }
  return findings;
}

async function loadPolicy(path) {
  const policy = JSON.parse(await readFile(path, "utf8"));
  if (policy.version !== 1 || !Array.isArray(policy.trackedDatabaseFixtures)) {
    throw new Error("artifact policy must be version 1 with trackedDatabaseFixtures");
  }
  const fixturePaths = new Set();
  for (const fixture of policy.trackedDatabaseFixtures) {
    if (!fixture.path || !fixture.manifest || isAbsolute(fixture.path) || isAbsolute(fixture.manifest)) {
      throw new Error("fixture policy paths must be non-empty repository-relative paths");
    }
    const fixturePath = normalizePath(fixture.path);
    const manifestPath = normalizePath(fixture.manifest);
    if (
      fixturePath.startsWith("../") ||
      manifestPath.startsWith("../") ||
      !/\.(?:db|sqlite|sqlite3)$/i.test(fixturePath)
    ) {
      throw new Error("fixture policy may allow only canonical repository-relative database files");
    }
    if (fixturePaths.has(fixturePath)) throw new Error(`duplicate fixture policy path: ${fixturePath}`);
    fixturePaths.add(fixturePath);
    if (!Array.isArray(fixture.allowedSensitiveValues ?? [])) {
      throw new Error(`fixture allowedSensitiveValues must be an array: ${fixturePath}`);
    }
  }
  return policy;
}

export async function main(argv = process.argv.slice(2)) {
  const options = parseArguments(argv);
  const policy = await loadPolicy(options.policyPath);
  const canaries = [
    ...options.canaries,
    ...(process.env.RELAY_POOL_ARTIFACT_SCAN_CANARIES ?? "")
      .split(sep === "\\" ? ";" : ":")
      .filter(Boolean),
  ];
  const findings = [];
  if (options.tracked) {
    findings.push(...(await scanTrackedIndex(options.repoRoot, policy, canaries)));
  }
  for (const sqlite of options.sqlite) {
    const sqliteFindings = await inspectSqlite(sqlite, [], canaries);
    findings.push(...sqliteFindings.map((finding) => `${basename(sqlite)}: ${finding}`));
  }
  for (const artifact of options.artifacts) {
    findings.push(...(await scanArtifactPath(artifact, { canaries, repoRoot: options.repoRoot })));
  }

  if (findings.length > 0) {
    console.error("Persistence V2 artifact scan failed:");
    for (const finding of findings) console.error(`- ${finding}`);
    return 1;
  }
  console.log("Persistence V2 artifact scan passed.");
  return 0;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    process.exitCode = await main();
  } catch (error) {
    console.error(`Persistence V2 artifact scan failed: ${error.message}`);
    process.exitCode = 1;
  }
}
