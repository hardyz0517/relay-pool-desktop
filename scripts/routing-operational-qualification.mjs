import fs from "node:fs";
import path from "node:path";
import { assert, currentRevision, readJson, repoRoot, runMain } from "./architecture/lib.mjs";

const MANIFEST_PATH = "docs/superpowers/audits/routing-operational-qualification-manifest.json";

function hasFlag(name) {
  return process.argv.includes(name);
}

function normalize(value) {
  return value.replaceAll("\\", "/");
}

function readArtifact(relativePath, label) {
  const absolute = path.join(repoRoot, relativePath);
  assert(fs.existsSync(absolute), `missing ${label}: ${relativePath}`);
  return JSON.parse(fs.readFileSync(absolute, "utf8"));
}

function assertSoakScriptCoversSuites(manifest) {
  const source = fs.readFileSync(path.join(repoRoot, "scripts/run-routing-operational-soak.ps1"), "utf8");
  for (const suite of manifest.required_soak_suites) {
    assert(source.includes(`"${suite}"`), `soak script does not run required suite ${suite}`);
  }
  for (const text of [
    "schemaVersion = 1",
    "sourceRevision",
    "worktreeCleanAtStart",
    "worktreeCleanAtFinish",
    "thresholds",
    "environment",
    "candidateLimit = 1024",
    "maxRuntimeReplans = 8",
  ]) {
    assert(source.includes(text), `soak script is missing report contract text: ${text}`);
  }
}

function assertVerifyPreflightIsWired() {
  const source = fs.readFileSync(path.join(repoRoot, "scripts/verify.ps1"), "utf8");
  assert(
    source.includes('Invoke-Checked "Routing operational self-check preflight" node @("scripts/routing-operational-qualification.mjs", "--preflight")'),
    "verify.ps1 full profile must run routing operational self-check preflight",
  );
}

function assertManifestShape(manifest) {
  assert(manifest.schema_version === 1, "self-check manifest schema_version must be 1");
  assert(manifest.owner_task === 26, "self-check manifest owner_task must be 26");
  assert(Array.isArray(manifest.required_development_commands), "manifest requires development commands");
  assert(Array.isArray(manifest.optional_aggregate_commands), "manifest requires optional aggregate commands");
  assert(Array.isArray(manifest.optional_confidence_commands), "manifest requires optional confidence commands");
  assert(
    new Set(manifest.required_development_commands).size === manifest.required_development_commands.length,
    "required development commands must not contain duplicates",
  );
  for (const command of [
    "pnpm.cmd architecture:fixtures",
    "pnpm.cmd architecture:typescript",
    "pnpm.cmd architecture:commands",
    "pnpm.cmd architecture:security",
    "pnpm.cmd architecture:artifacts",
    "pnpm.cmd test:contracts",
    "pnpm.cmd build",
    "cargo check --locked --manifest-path src-tauri/Cargo.toml",
    "pnpm.cmd architecture:scale-baseline",
    "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -Smoke",
    "node scripts/routing-operational-qualification.mjs",
  ]) {
    assert(
      manifest.required_development_commands.includes(command),
      `manifest required development commands missing ${command}`,
    );
  }
  assert(
    !manifest.required_development_commands.includes("pnpm.cmd verify:full"),
    "verify:full is optional and must not be part of required development commands",
  );
  assert(
    manifest.optional_aggregate_commands.includes("pnpm.cmd verify:full"),
    "verify:full should remain documented as an optional aggregate check",
  );
  for (const command of [
    "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-routing-operational-soak.ps1 -DurationMinutes 60",
    "node scripts/routing-operational-qualification.mjs --require-long-soak",
  ]) {
    assert(
      manifest.optional_confidence_commands.includes(command),
      `manifest optional confidence commands missing ${command}`,
    );
  }
  assert(Array.isArray(manifest.required_soak_suites), "manifest requires soak suites");
  assert(manifest.required_soak_suites.length >= 10, "manifest must cover the final routing/proxy fault mix");
  assert(manifest.required_thresholds?.required_default_duration_minutes === 0, "manifest default deterministic soak must remain single-pass/smoke");
  assert(manifest.required_thresholds?.optional_confidence_duration_minutes === 60, "manifest optional confidence soak duration must remain 60 minutes");
  assert(manifest.required_thresholds?.failures === 0, "manifest must require zero failures");
  assert(manifest.required_thresholds?.candidate_limit === 1024, "manifest must preserve candidate limit");
  assert(manifest.required_thresholds?.max_runtime_replans === 8, "manifest must preserve max runtime replans");
  for (const [name, relativePath] of Object.entries(manifest.artifact_inputs ?? {})) {
    assert(typeof relativePath === "string" && normalize(relativePath).startsWith("output/"), `manifest artifact path ${name} must be scoped under output/`);
  }
}

function assertScaleBaselineIfPresent(manifest) {
  if (!manifest.artifact_inputs?.scale_baseline_report) {
    return;
  }
  const absolute = path.join(repoRoot, manifest.artifact_inputs.scale_baseline_report);
  if (!fs.existsSync(absolute)) {
    return;
  }
  const report = readArtifact(manifest.artifact_inputs.scale_baseline_report, "scale baseline report");
  const fixturesPath = path.join(repoRoot, manifest.artifact_inputs.scale_baseline_fixtures);
  assert(fs.existsSync(fixturesPath), `missing scale baseline fixture manifest: ${manifest.artifact_inputs.scale_baseline_fixtures}`);
  assert(report.schema_version === 1, "scale baseline report schema_version must be 1");
  assert(
    report.provenance?.source_revision === currentRevision(),
    "scale baseline source_revision must match current HEAD; rerun architecture:scale-baseline after tracked changes",
  );
}

function assertSoakReport(manifest, requireLongSoak) {
  const report = readArtifact(manifest.artifact_inputs.soak_report, "routing operational soak report");
  assert(report.schemaVersion === 1, "soak report schemaVersion must be 1");
  assert(report.kind === "routing-operational-loopback-soak", "unexpected soak report kind");
  assert(typeof report.sourceRevision === "string" && report.sourceRevision.length >= 7, "soak report must include sourceRevision");
  assert(report.sourceRevision === currentRevision(), "soak report sourceRevision must match current HEAD; rerun the deterministic soak after tracked changes");
  assert(typeof report.worktreeCleanAtStart === "boolean", "soak report must record worktreeCleanAtStart");
  assert(typeof report.worktreeCleanAtFinish === "boolean", "soak report must record worktreeCleanAtFinish");
  assert(Array.isArray(report.failures) && report.failures.length === 0, "soak report contains failures");
  assert(Number.isInteger(report.iterations) && report.iterations >= 1, "soak requires at least one iteration");
  assert(report.thresholds?.candidateLimit === manifest.required_thresholds.candidate_limit, "soak candidate limit drifted");
  assert(report.thresholds?.maxRuntimeReplans === manifest.required_thresholds.max_runtime_replans, "soak max replans drifted");
  if (requireLongSoak) {
    assert(report.smoke === false, "long confidence soak must not use smoke mode");
    assert(
      report.requestedDurationMinutes >= manifest.required_thresholds.optional_confidence_duration_minutes,
      "long confidence soak duration is below optional threshold",
    );
  } else {
    assert(
      report.smoke === true || report.requestedDurationMinutes >= manifest.required_thresholds.required_default_duration_minutes,
      "development self-check requires at least the default single-pass deterministic soak",
    );
  }
  for (const suite of manifest.required_soak_suites) {
    assert(
      report.commandPlan?.some((commandLine) => commandLine.includes(`--test ${suite}`)),
      `soak report did not include required suite ${suite}`,
    );
  }
}

runMain(() => {
  const preflight = hasFlag("--preflight");
  const requireLongSoak = hasFlag("--require-long-soak");
  const manifest = readJson(MANIFEST_PATH, "routing operational self-check manifest");
  assertManifestShape(manifest);
  assertSoakScriptCoversSuites(manifest);
  assertVerifyPreflightIsWired();

  if (preflight) {
    console.log("routing operational self-check preflight passed");
    return;
  }

  assertSoakReport(manifest, requireLongSoak);
  assertScaleBaselineIfPresent(manifest);
  console.log("routing operational development self-check artifacts passed");
});
