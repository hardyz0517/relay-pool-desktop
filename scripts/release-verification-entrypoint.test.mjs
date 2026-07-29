import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { parse as parseYaml } from "yaml";

const pkg = JSON.parse(await readFile("package.json", "utf8"));
const workflow = parseYaml(await readFile(".github/workflows/release.yml", "utf8"));
const verifier = await readFile("scripts/verify.ps1", "utf8");
const steps = workflow.jobs.release.steps;
const actionIndex = steps.findIndex((step) => String(step.uses ?? "").startsWith("tauri-apps/tauri-action@"));
const prebundleIndex = steps.findIndex((step) => String(step.run ?? "").includes("-Profile release -ReleasePhase prebundle"));
const postbundleIndex = steps.findIndex((step) => String(step.run ?? "").includes("-Profile release -ReleasePhase postbundle"));
const tagCheckIndex = steps.findIndex((step) => String(step.run ?? "").includes("verify:release-version --require-tag"));

assert.equal(pkg.scripts["test:contracts"], "node scripts/run-contract-tests.mjs");
assert.equal(pkg.scripts["verify:release-version"], "node scripts/verify-release-version.mjs");
assert.equal(pkg.scripts["verify:persistence-artifacts"], "node scripts/verify-persistence-v2-artifacts.mjs --tracked");
assert.match(pkg.scripts["verify:release-bundle"], /verify-persistence-v2-artifacts\.mjs --artifact/);
assert.equal(pkg.scripts["verify:release"], "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify.ps1 -Profile release");

for (const required of [
  "verify:persistence-artifacts",
  "architecture:scale-baseline",
  "test:contracts",
  "Frontend unit tests",
  "Frontend production build",
  "cargo",
  "--locked",
  "verify:release-bundle",
]) {
  assert.ok(verifier.includes(required), `shared verifier is missing ${required}`);
}

assert.ok(tagCheckIndex >= 0 && prebundleIndex > tagCheckIndex, "tag/source mismatch must fail before full prebundle verification");
assert.ok(
  !steps.some((step) => String(step.run ?? "").includes("verify:release-version -- --require-tag")),
  "release workflow must not forward a literal -- to the release version script",
);
assert.ok(actionIndex > prebundleIndex, "signed packaging must run only after shared prebundle verification");
assert.ok(postbundleIndex > actionIndex, "final artifact scan must run after Tauri packaging");
assert.equal(steps[actionIndex].with.tagName, "${{ github.ref_name }}");
assert.equal(steps[actionIndex].with.releaseName, "Relay Pool Desktop ${{ github.ref_name }}");
assert.equal(steps[actionIndex].with.releaseBody, "${{ steps.release_notes.outputs.body }}");
assert.equal(steps[actionIndex].with.releaseDraft, true);
assert.ok(steps.some((step) => String(step.uses ?? "").startsWith("actions/setup-python@")));
assert.ok(!steps.some((step) => String(step.run ?? "").includes("node scripts/updater-")));

console.log("release verification entrypoint contract checks passed");
