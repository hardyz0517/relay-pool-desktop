import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";

const script = await readFile("scripts/build-persistence-v1-performance-baseline.ps1", "utf8");
assert.match(
  script,
  /function Get-Sha256Text \{\s*param\(\[Parameter\(Mandatory = \$true\)\]\[AllowEmptyString\(\)\]\[string\] \$Text\)/,
  "the controlled worktree snapshot must hash empty clean-status and diff content",
);
assert.match(
  script,
  /persistence_v1_performance_probe::reconstructed_v031_baseline' -- --nocapture --test-threads=1/,
  "the appended probe must be selected by its stable suffix rather than an unknown enclosing module path",
);
assert.match(
  script,
  /V1 performance probe source changed during baseline measurement/,
  "the baseline must reject changes to the appended V1 probe source",
);
assert.match(
  script,
  /V1 baseline build changed unexpected tracked paths/,
  "the baseline must reject build mutations outside the known generated schemas and appended probe",
);
assert.match(
  script,
  /\[regex\]::Match\(\(\$powerOutput \| Out-String\), '\[0-9a-fA-F\]\{8\}/,
  "the baseline environment must capture the locale-independent power-scheme GUID",
);

const result = spawnSync(
  "powershell",
  [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    "scripts/build-persistence-v1-performance-baseline.ps1",
    "-ValidateInputsOnly",
  ],
  { cwd: process.cwd(), encoding: "utf8", shell: false },
);

assert.equal(
  result.status,
  0,
  `V1 baseline input validation failed:\nstdout=${result.stdout}\nstderr=${result.stderr}`,
);
const validation = JSON.parse(result.stdout.trim());
assert.equal(validation.status, "inputs-validated-not-measured");
assert.equal(validation.baselineKind, "reconstructed-v0.3.1-source-baseline");
assert.equal(validation.releaseCommit, "54751559aed8f3f7c159e322bc7bbcc71d993204");
assert.equal(
  validation.releasedFixtureSha256,
  "ad1f159cd6feabbb7d9bb4d6a37bf4fbc979f98eab03a42f402eb6fa863f34c9",
);
assert.match(validation.benchmarkProbeSha256, /^[a-f0-9]{64}$/);
assert.equal(validation.workloads.requestLogs.rows, 500);
assert.equal(
  validation.workloads.requestLogs.projection,
  "v0.3.1-production-full-row-representative-economics-attempt-model",
);
assert.equal(validation.workloads.changeEvents.queryLimit, 201);
assert.equal(validation.workloads.changeEvents.returnedRows, 200);
assert.equal(
  validation.workloads.changeEvents.projection,
  "v0.3.1-production-full-row-representative-associated-fields",
);
assert.equal(validation.workloads.startup.migrationsIncluded, false);
assert.deepEqual(validation.standardFixture, {
  stations: 100,
  stationKeys: 1000,
  requestLogs: 10000,
  changeEvents: 100000,
});
