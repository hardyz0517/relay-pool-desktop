import { spawnSync } from "node:child_process";
import { assert, readJson, repoRoot, runMain } from "./lib.mjs";

runMain(() => {
  // Node cannot execute .cmd shims directly on all supported Windows releases.
  // The command is repository-owned and contains no interpolated input.
  const executable = process.platform === "win32" ? (process.env.ComSpec ?? "cmd.exe") : "pnpm";
  const args = process.platform === "win32"
    ? ["/d", "/s", "/c", "pnpm.cmd audit --prod --json"]
    : ["audit", "--prod", "--json"];
  const result = spawnSync(executable, args, {
    cwd: repoRoot,
    encoding: "utf8",
    windowsHide: true,
  });
  assert(!result.error, `pnpm audit failed to start: ${result.error?.message}`);
  let report;
  try {
    report = JSON.parse(result.stdout);
  } catch {
    throw new Error(`pnpm audit did not return JSON: ${result.stderr || result.stdout}`);
  }
  const exceptions = readJson("docs/audits/dependency-advisory-exceptions.json").exceptions
    .filter((entry) => entry.ecosystem === "npm");
  const allowed = new Set(exceptions.map((entry) => `${entry.package}:${entry.advisory_id}`));
  const legacyAdvisories = Object.entries(report.advisories ?? {}).map(([id, advisory]) => ({
    id: advisory.github_advisory_id ?? advisory.id ?? id,
    package: advisory.module_name ?? advisory.package ?? advisory.name,
    severity: String(advisory.severity ?? "unknown").toLowerCase(),
  }));
  const modernAdvisories = Object.values(report.vulnerabilities ?? {}).flatMap((vulnerability) =>
    (vulnerability.via ?? []).filter((via) => via && typeof via === "object").map((via) => ({
      id: String(via.url ?? "").match(/GHSA-[\w-]+/i)?.[0] ?? String(via.source ?? "unknown"),
      package: vulnerability.name ?? via.name,
      severity: String(via.severity ?? vulnerability.severity ?? "unknown").toLowerCase(),
    })),
  );
  const advisories = [...legacyAdvisories, ...modernAdvisories];
  const blocking = advisories.filter((advisory) => ["high", "critical"].includes(advisory.severity) && !allowed.has(`${advisory.package}:${advisory.id}`));
  assert(blocking.length === 0, `blocking npm advisories: ${blocking.map((entry) => `${entry.package}:${entry.id}:${entry.severity}`).join(", ")}`);
  if (result.status !== 0) {
    const reportedHigh = advisories.some((advisory) => ["high", "critical"].includes(advisory.severity));
    assert(reportedHigh, `pnpm audit failed without parseable high/critical advisories: ${result.stderr}`);
  }
  console.log(`npm advisory gate passed (${advisories.length} advisories inspected)`);
});
