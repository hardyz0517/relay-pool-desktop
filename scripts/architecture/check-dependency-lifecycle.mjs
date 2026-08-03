import fs from "node:fs";
import path from "node:path";
import { parse as parseYaml } from "yaml";
import {
  assert,
  command,
  parseIsoDate,
  readRequiredManifest,
  repoRoot,
  runMain,
} from "./lib.mjs";

const REQUIRED = new Map([
  ["npm", ["react", "vite", "@tauri-apps/api", "@tanstack/react-query", "typescript", "eslint", "@eslint/js", "typescript-eslint", "yaml"]],
  ["cargo", ["tauri", "tokio", "reqwest", "axum", "sqlx", "syn", "age", "hmac"]],
  ["tool", ["node", "pnpm", "rust"]],
]);

function cargoLockPackages() {
  const lockfile = fs.readFileSync(path.join(repoRoot, "src-tauri/Cargo.lock"), "utf8");
  const packages = new Map();
  for (const section of lockfile.split(/\r?\n\[\[package\]\]\r?\n/).slice(1)) {
    const name = section.match(/^name = "([^"]+)"/m)?.[1];
    const version = section.match(/^version = "([^"]+)"/m)?.[1];
    if (!name || !version) continue;
    const versions = packages.get(name) ?? [];
    versions.push(version);
    packages.set(name, versions);
  }
  return packages;
}

function resolvedVersions() {
  const packageJson = JSON.parse(fs.readFileSync(path.join(repoRoot, "package.json"), "utf8"));
  const pnpmExecutable = process.platform === "win32" ? "pnpm.cmd" : "pnpm";
  const npmList = process.platform === "win32"
    ? command(process.env.ComSpec ?? "cmd.exe", ["/d", "/s", "/c", `${pnpmExecutable} list --depth 0 --json`])
    : command(pnpmExecutable, ["list", "--depth", "0", "--json"]);
  const npmTree = JSON.parse(npmList);
  const npmRoot = Array.isArray(npmTree) ? npmTree[0] : npmTree;
  const npm = new Map(Object.entries({ ...(npmRoot.dependencies ?? {}), ...(npmRoot.devDependencies ?? {}) })
    .map(([name, value]) => [name, value.version]));

  const cargoPackages = cargoLockPackages();
  const tools = new Map([
    ["node", process.versions.node],
    ["pnpm", packageJson.packageManager?.split("@").at(-1)],
    ["rust", command("rustc", ["--version"]).trim().split(/\s+/)[1]],
  ]);
  return new Map([["npm", npm], ["cargo", cargoPackages], ["tool", tools]]);
}

function entriesOf(ledger) {
  const raw = ledger.components ?? ledger.dependencies ?? ledger.entries;
  assert(Array.isArray(raw), "dependency lifecycle ledger must contain a components array");
  return raw;
}

function normalizedStatus(entry) {
  return String(entry.support_status ?? entry.status ?? "").toLowerCase();
}

function workflowNodeVersions() {
  const versions = new Map();
  for (const workflow of [".github/workflows/ci.yml", ".github/workflows/release.yml"]) {
    const parsed = parseYaml(fs.readFileSync(path.join(repoRoot, workflow), "utf8"));
    const steps = Object.values(parsed.jobs ?? {}).flatMap((job) => job.steps ?? []);
    const setup = steps.filter((step) => typeof step.uses === "string" && step.uses.startsWith("actions/setup-node@"));
    assert(setup.length === 1, `${workflow} must contain exactly one pinned setup-node step`);
    const version = String(setup[0].with?.["node-version"] ?? "");
    assert(/^\d+\.\d+\.\d+$/.test(version), `${workflow} setup-node must use an exact semver`);
    versions.set(workflow, version);
  }
  return versions;
}

function workflowRustVersions() {
  const versions = new Map();
  for (const workflow of [".github/workflows/ci.yml", ".github/workflows/release.yml"]) {
    const parsed = parseYaml(fs.readFileSync(path.join(repoRoot, workflow), "utf8"));
    for (const [jobName, job] of Object.entries(parsed.jobs ?? {})) {
      if (jobName === "msrv") continue;
      const setup = (job.steps ?? []).filter((step) => typeof step.uses === "string" && step.uses.startsWith("dtolnay/rust-toolchain@"));
      assert(setup.length === 1, `${workflow}:${jobName} must contain exactly one pinned Rust toolchain step`);
      const toolchain = String(setup[0].with?.toolchain ?? "");
      const version = toolchain.match(/^(\d+\.\d+\.\d+)(?:-.+)?$/)?.[1] ?? "";
      assert(version, `${workflow}:${jobName} Rust toolchain must use an exact semver`);
      versions.set(`${workflow}:${jobName}`, version);
    }
  }
  return versions;
}

runMain(() => {
  const ledger = readRequiredManifest("docs/superpowers/audits/architecture-scale-dependency-lifecycle.json", ["schema_version"]);
  assert(ledger.schema_version === 1, "dependency lifecycle schema_version must be 1");
  const entries = entriesOf(ledger);
  const actual = resolvedVersions();
  const workflowNodes = workflowNodeVersions();
  const workflowRust = workflowRustVersions();
  const now = new Date();
  const localDate = [
    now.getFullYear(),
    String(now.getMonth() + 1).padStart(2, "0"),
    String(now.getDate()).padStart(2, "0"),
  ].join("-");
  const today = parseIsoDate(localDate, "local calendar date");

  const indexed = new Map();
  for (const [index, entry] of entries.entries()) {
    assert(entry && typeof entry === "object", `components[${index}] must be an object`);
    const ecosystem = String(entry.ecosystem ?? "").toLowerCase();
    const name = entry.package ?? entry.component ?? entry.name;
    assert(actual.has(ecosystem), `components[${index}] has unknown ecosystem '${ecosystem}'`);
    assert(typeof name === "string" && name.trim(), `components[${index}] requires package/component/name`);
    assert(!indexed.has(`${ecosystem}:${name}`), `duplicate dependency lifecycle entry ${ecosystem}:${name}`);
    indexed.set(`${ecosystem}:${name}`, entry);

    const status = normalizedStatus(entry);
    assert(status && !/(?:unknown|unresolved|unsupported|eol|block)/.test(status), `${ecosystem}:${name} has blocking support status '${status || "missing"}'`);
    const reviewed = parseIsoDate(entry.checked_on ?? entry.reviewed_on ?? entry.check_date, `${ecosystem}:${name}.checked_on`);
    const nextReview = parseIsoDate(entry.next_review_on ?? entry.next_review_date ?? entry.next_review, `${ecosystem}:${name}.next_review_on`);
    assert(nextReview > today, `${ecosystem}:${name} dependency review is expired`);
    assert(reviewed <= today, `${ecosystem}:${name} review date is in the future`);
    const sources = entry.source_urls ?? entry.sources ?? [entry.source_url ?? entry.source];
    assert(Array.isArray(sources) && sources.length > 0 && sources.every((source) => typeof source === "string" && /^https:\/\//.test(source)), `${ecosystem}:${name} requires traceable HTTPS sources`);
    assert(typeof entry.owner === "string" && entry.owner.trim(), `${ecosystem}:${name}.owner is required`);
    assert(typeof entry.decision === "string" && /^(?:keep|upgrade|replace|block)$/i.test(entry.decision), `${ecosystem}:${name}.decision is invalid`);

    const resolved = actual.get(ecosystem).get(name);
    assert(resolved, `${ecosystem}:${name} is not present in the resolved toolchain/lockfile`);
    const recorded = String(entry.resolved_version ?? entry.version ?? "").replace(/^v/, "");
    if (ecosystem === "tool" && name === "node") {
      assert(Array.isArray(entry.qualified_versions) && entry.qualified_versions.length >= 2, "tool:node requires exact qualified_versions for CI and local reference runtimes");
      assert(entry.qualified_versions.every((version) => /^\d+\.\d+\.\d+$/.test(version)), "tool:node qualified_versions must be exact semver values");
      assert(entry.qualified_versions.includes(String(resolved).replace(/^v/, "")), `tool:node current ${resolved} is not a qualified runtime`);
      assert(entry.local_reference_version === recorded, "tool:node resolved_version must identify local_reference_version");
      assert(entry.qualified_versions.includes(entry.ci_version), "tool:node ci_version must be qualified");
      for (const [workflow, version] of workflowNodes) assert(version === entry.ci_version, `${workflow} Node ${version} != ledger ci_version ${entry.ci_version}`);
    } else if (ecosystem === "tool" && name === "rust") {
      assert(Array.isArray(entry.qualified_versions) && entry.qualified_versions.length >= 2, "tool:rust requires exact qualified_versions for CI and local reference runtimes");
      assert(entry.qualified_versions.every((version) => /^\d+\.\d+\.\d+$/.test(version)), "tool:rust qualified_versions must be exact semver values");
      assert(entry.qualified_versions.includes(String(resolved).replace(/^v/, "")), `tool:rust current ${resolved} is not a qualified runtime`);
      assert(entry.local_reference_version === recorded, "tool:rust resolved_version must identify local_reference_version");
      assert(entry.qualified_versions.includes(entry.ci_version), "tool:rust ci_version must be qualified");
      for (const [workflow, version] of workflowRust) assert(version === entry.ci_version, `${workflow} Rust ${version} != ledger ci_version ${entry.ci_version}`);
    } else {
      const resolvedVersions = Array.isArray(resolved) ? resolved : [String(resolved)];
      assert(
        resolvedVersions.map((version) => String(version).replace(/^v/, "")).includes(recorded),
        `${ecosystem}:${name} ledger version ${recorded} is not in resolved versions ${resolvedVersions.join(", ")}`,
      );
    }
    if (/^upgrade$/i.test(entry.decision)) {
      const prerequisite = entry.prerequisite_shard;
      assert(prerequisite && typeof prerequisite === "object", `${ecosystem}:${name} upgrade requires prerequisite_shard`);
      for (const key of ["id", "compatibility_matrix", "rollback_revision", "qualification"]) {
        assert(prerequisite[key], `${ecosystem}:${name}.prerequisite_shard.${key} is required`);
      }
    }
  }

  for (const [ecosystem, packages] of REQUIRED) {
    for (const name of packages) assert(indexed.has(`${ecosystem}:${name}`), `dependency lifecycle ledger is missing critical component ${ecosystem}:${name}`);
  }
  console.log(`Dependency lifecycle gate passed (${entries.length} entries)`);
});
