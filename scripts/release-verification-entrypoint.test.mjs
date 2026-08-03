import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { parse as parseYaml } from "yaml";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const workflow = parseYaml(await readFile(".github/workflows/release.yml", "utf8"));
const verifier = await readFile("scripts/verify.ps1", "utf8");
const securityPolicy = await readFile("docs/SECURITY_EXPORT_IMPORT.md", "utf8");
const portableMigrationAdr = await readFile("docs/superpowers/specs/2026-07-29-portable-migration-crypto-format-adr.md", "utf8");
const portableMigrationChecklist = await readFile("docs/release/PORTABLE_MIGRATION_SMOKE_CHECKLIST.md", "utf8");
const portableMigrationFacade = await readFile("src-tauri/src/application/data_migration/mod.rs", "utf8");
const steps = workflow.jobs.release.steps;
const checkoutStep = steps.find((step) => String(step.uses ?? "").startsWith("actions/checkout@"));
const actionIndex = steps.findIndex((step) => String(step.uses ?? "").startsWith("tauri-apps/tauri-action@"));
const prebundleIndex = steps.findIndex((step) => String(step.run ?? "").includes("pnpm verify:release:prebundle"));
const postbundleIndex = steps.findIndex((step) => String(step.run ?? "").includes("pnpm verify:release:postbundle"));
const tagCheckIndex = steps.findIndex((step) => String(step.run ?? "").includes("verify:release-version --require-tag"));
const releaseVersionGateIndex = verifier.indexOf('Invoke-Checked "Release version contract"');
const sharedGateStartIndex = verifier.indexOf("Invoke-ArchitectureGates", verifier.indexOf("verify start="));

assert.equal(pkg.scripts["test:contracts"], "node scripts/run-contract-tests.mjs");
assert.equal(pkg.scripts["verify:release-version"], "node scripts/verify-release-version.mjs");
assert.equal(pkg.scripts["verify:persistence-artifacts"], "node scripts/verify-persistence-v2-artifacts.mjs --tracked");
assert.match(pkg.scripts["verify:release-bundle"], /verify-persistence-v2-artifacts\.mjs --artifact/);
assert.equal(pkg.scripts["test:dead-code-policy"], "node scripts/dead-code-inventory-policy.test.mjs");
assert.equal(pkg.scripts["verify:release"], "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1 -Profile release");
assert.equal(
  pkg.scripts["verify:release:prebundle"],
  "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1 -Profile release -ReleasePhase prebundle",
);
assert.equal(
  pkg.scripts["verify:release:postbundle"],
  "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1 -Profile release -ReleasePhase postbundle",
);

for (const required of [
  "verify:persistence-artifacts",
  "architecture:scale-baseline",
  "test:contracts",
  "Frontend unit tests",
  "Frontend production build",
  "Dead code policy fixtures",
  "Dead code CI policy",
  "dead-code-inventory.mjs",
  "cargo",
  "--locked",
  "--all-targets",
  "--release",
  "--lib",
  "verify:release-bundle",
]) {
  assert.ok(verifier.includes(required), `shared verifier is missing ${required}`);
}

for (const nonGate of [
  "Portable migration e2e integration",
  "portable_migration_faults",
  "portable_migration_malicious",
]) {
  assert.ok(
    !verifier.includes(nonGate),
    `portable migration qualification should stay out of the shared release gate during the current pre-stable product phase: ${nonGate}`,
  );
}

assert.equal(tagCheckIndex, -1, "release workflow must route release tag/source checks through the shared verifier");
assert.ok(
  releaseVersionGateIndex >= 0 && sharedGateStartIndex > releaseVersionGateIndex,
  "tag/source mismatch must fail inside the shared verifier before full prebundle verification",
);
assert.ok(
  !steps.some((step) => String(step.run ?? "").includes("verify:release-version -- --require-tag")),
  "release workflow must not forward a literal -- to the release version script",
);
assert.ok(
  !steps.some((step) => String(step.run ?? "").includes("verify:release -- -ReleasePhase")),
  "release workflow must not forward a literal -- to the PowerShell release verifier",
);
assert.equal(
  checkoutStep?.with?.["fetch-depth"],
  0,
  "release verification needs historical tags for the immutable v0.3.1 baseline",
);
assert.ok(
  !verifier.includes('"tauri:build", "--", "--target"'),
  "shared verifier must not forward a literal -- to the Tauri build script",
);
assert.match(portableMigrationFacade, /SECURITY_POLICY_APPROVED:\s*bool\s*=\s*true/, "portable migration must be enabled only with a documented security approval");
assert.match(securityPolicy, /Security approval: approved by the repository owner on 2026-07-30/, "security policy must document the approval record before enabling portable migration");
assert.match(securityPolicy, /Release promotion still requires the two-machine smoke checklist/, "security policy must keep approval separate from release qualification");
assert.match(securityPolicy, /A lost migration password is unrecoverable/, "security policy must document unrecoverable migration passwords");
assert.match(portableMigrationAdr, /Approval enables the branch capability; it does not by itself satisfy release qualification/, "ADR must keep approval separate from release qualification");
assert.match(portableMigrationChecklist, /Windows 10\/11 virtual machines/, "smoke checklist must require two-machine Windows qualification");
assert.match(portableMigrationChecklist, /run-portable-migration-performance\.ps1/, "smoke checklist must record the portable migration performance harness");
assert.ok(actionIndex > prebundleIndex, "signed packaging must run only after shared prebundle verification");
assert.ok(postbundleIndex > actionIndex, "final artifact scan must run after Tauri packaging");
assert.equal(steps[actionIndex].with.tagName, "${{ github.ref_name }}");
assert.equal(steps[actionIndex].with.releaseName, "Relay Pool Desktop ${{ github.ref_name }}");
assert.equal(steps[actionIndex].with.releaseBody, "${{ steps.release_notes.outputs.body }}");
assert.equal(steps[actionIndex].with.releaseDraft, true);
assert.ok(steps.some((step) => String(step.uses ?? "").startsWith("actions/setup-python@")));
assert.ok(!steps.some((step) => String(step.run ?? "").includes("node scripts/updater-")));

console.log("release verification entrypoint contract checks passed");
