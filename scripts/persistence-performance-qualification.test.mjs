import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = await mkdtemp(join(tmpdir(), "relay-pool-performance-contract-"));
const baselinePath = join(root, "baseline.json");
const v2Path = join(root, "v2-output.txt");
const outputPath = join(root, "evidence.json");
const schemaFixturePath = join(
  process.cwd(),
  "scripts",
  "fixtures",
  "persistence-performance-v2-release-schema.json",
);
const qualificationWrapperPath = join(
  process.cwd(),
  "scripts",
  "run-persistence-performance-qualification.ps1",
);
const rustQualificationOutputPath =
  process.env.PERSISTENCE_RUST_QUALIFICATION_OUTPUT;
const baseline = {
  schemaVersion: 1,
  baselineKind: "reconstructed-v0.3.1-source-baseline",
  provenance: {
    releaseCommit: "54751559aed8f3f7c159e322bc7bbcc71d993204",
    releasedFixtureSha256: "ad1f159cd6feabbb7d9bb4d6a37bf4fbc979f98eab03a42f402eb6fa863f34c9",
    derivedFixtureSha256: "a".repeat(64),
    benchmarkProbeSha256: "b".repeat(64),
  },
  build: { profile: "release", locked: true },
  environment: {
    cpuModel: "contract CPU",
    logicalProcessors: 16,
    installedMemoryBytes: 16_000_000_000,
    windowsCaption: "contract Windows",
    windowsVersion: "10.0.1",
    windowsBuild: "1",
    activePowerScheme: "contract power scheme",
    rustcVersion: "rustc contract",
    cargoVersion: "cargo contract",
    gitHead: "54751559aed8f3f7c159e322bc7bbcc71d993204",
    worktreeDirty: true,
    worktreeStatusSha256: "c".repeat(64),
    worktreeSnapshot: {
      kind: "hashed-dirty-worktree",
      trackedDiffSha256: "d".repeat(64),
      untrackedContentSha256: "e".repeat(64),
    },
    antivirusProducts: [],
    defenderRealTimeProtection: "unavailable",
    windowsSearchService: "unavailable",
  },
  standardFixture: {
    stations: 100,
    stationKeys: 1000,
    requestLogs: 10000,
    changeEvents: 100000,
  },
  workloads: {
    requestLogs: {
      rows: 500,
      projection: "v0.3.1-production-full-row-representative-economics-attempt-model",
    },
    changeEvents: {
      queryLimit: 201,
      returnedRows: 200,
      projection: "v0.3.1-production-full-row-representative-associated-fields",
      contract: "normalized-first-page-not-v0.3.1-public-api",
    },
    startup: { migrationsIncluded: false },
  },
  metrics: {
    hotRequestLogs: { samplesNs: Array.from({ length: 40 }, (_, index) => 100 + index) },
    hotChangeEventsFirstPage: {
      samplesNs: Array.from({ length: 40 }, (_, index) => 100 + index),
    },
    startupWithoutMigration: {
      samplesNs: Array.from({ length: 15 }, (_, index) => 100 + index),
    },
  },
  measurement: {
    startedAtUtc: "2026-07-21T00:00:00.0000000Z",
    completedAtUtc: "2026-07-21T00:10:00.0000000Z",
  },
};

try {
  const qualificationWrapperSource = await readFile(
    qualificationWrapperPath,
    "utf8",
  );
  assert.match(
    qualificationWrapperSource,
    /foreach \(\$field in @\('kind', 'gitHead'\)\)/,
    "V2 provenance snapshots must be compared by fields instead of JSON property order",
  );
  assert.match(
    qualificationWrapperSource,
    /\[regex\]::Match\(\(\$powerOutput \| Out-String\), '\[0-9a-fA-F\]\{8\}/,
    "V2 environment must capture the locale-independent power-scheme GUID",
  );
  assert.match(
    qualificationWrapperSource,
    /RedirectStandardOutput = \$true[\s\S]+RedirectStandardError = \$true/,
    "qualification wrapper must capture Cargo and libtest output without a PowerShell pipeline",
  );
  assert.match(
    qualificationWrapperSource,
    /StandardOutput\.ReadToEndAsync\(\)[\s\S]+StandardError\.ReadToEndAsync\(\)[\s\S]+WaitForExit\(\)/,
    "qualification wrapper must drain stdout and stderr concurrently before waiting",
  );
  assert.doesNotMatch(
    qualificationWrapperSource,
    /& cargo test[^\n]+2>&1/,
    "qualification wrapper must not regress to descendant-output-losing PowerShell capture",
  );
  assert.match(
    qualificationWrapperSource,
    /--skip mock_wrapper_contract_emits_reports_from_rust_builders/,
    "release qualification must exclude the mock report emitter",
  );
  assert.match(
    qualificationWrapperSource,
    /if \(\$captured\.exitCode -ne 0\)[\s\S]+throw "V2 performance qualification failed/,
    "qualification wrapper must fail closed on a non-zero Cargo exit code",
  );
  const schemaFixture = JSON.parse(await readFile(schemaFixturePath, "utf8"));
  assert.equal(schemaFixture.reports.length, 2, "real V2 schema fixture must carry both suites");
  const rustQualificationOutput = rustQualificationOutputPath
    ? await readFile(rustQualificationOutputPath, "utf8")
    : null;
  const qualificationReports = rustQualificationOutput
    ? rustQualificationOutput
        .split(/\r?\n/)
        .flatMap((line) => {
          const marker = "PERSISTENCE_QUALIFICATION ";
          const markerIndex = line.indexOf(marker);
          return markerIndex === -1
            ? []
            : [JSON.parse(line.slice(markerIndex + marker.length))];
        })
    : schemaFixture.reports;
  assert.equal(
    qualificationReports.length,
    2,
    "V2 contract input must carry exactly two Rust qualification reports",
  );
  await writeFile(baselinePath, `${JSON.stringify(baseline)}\n`, "utf8");
  await writeFile(
    v2Path,
    `cargo stderr warning: contract noise\nbuild output\n${qualificationReports
      .map((report, index) =>
        `${index === 0 ? "test mock_wrapper_contract ... " : ""}PERSISTENCE_QUALIFICATION ${JSON.stringify(report)}`,
      )
      .join("\n")}\n`,
    "utf8",
  );

  const result = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "scripts/run-persistence-performance-qualification.ps1",
      "-BaselinePath",
      baselinePath,
      "-OutputPath",
      outputPath,
      "-MockV2QualificationPath",
      v2Path,
    ],
    { cwd: process.cwd(), encoding: "utf8", shell: false },
  );
  assert.equal(
    result.status,
    0,
    `qualification wrapper failed:\nstdout=${result.stdout}\nstderr=${result.stderr}`,
  );

  const evidence = JSON.parse(await readFile(outputPath, "utf8"));
  assert.equal(evidence.schemaVersion, 1);
  assert.equal(evidence.evidenceKind, "mock-contract-validation");
  assert.equal(evidence.qualificationStatus, "unqualified-mock-input");
  assert.notEqual(evidence.evidenceKind, "paired-persistence-performance-qualification");
  assert.equal(evidence.baseline.baselineKind, "reconstructed-v0.3.1-source-baseline");
  assert.equal(evidence.v2.suite, "standard");
  assert.equal(evidence.rawQualificationLines.length, 2);
  assert.match(evidence.rawQualificationLines[0], /^PERSISTENCE_QUALIFICATION /);
  assert.equal(evidence.relativeGates.hotRequestLogs.quantile, "p95");
  assert.equal(evidence.relativeGates.hotChangeEventsFirstPage.quantile, "p95");
  assert.equal(evidence.relativeGates.startupWithoutMigration.quantile, "median");
  assert.equal(evidence.relativeGates.hotRequestLogs.passed, true);
  assert.equal(evidence.relativeGates.hotChangeEventsFirstPage.passed, true);
  assert.equal(evidence.relativeGates.startupWithoutMigration.passed, true);
  assert.deepEqual(evidence.measurementOrder.sequence, [
    "reconstructed-v0.3.1-source-baseline",
    "persistence-v2",
  ]);
  assert.equal(
    evidence.v2.provenance.worktreeSnapshot.kind,
    "hashed-dirty-worktree",
  );

  const rawQualificationResult = async (name, rawOutput) => {
    const path = join(root, `${name}.txt`);
    await writeFile(path, rawOutput, "utf8");
    return spawnSync(
      "powershell",
      [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        "scripts/run-persistence-performance-qualification.ps1",
        "-BaselinePath",
        baselinePath,
        "-OutputPath",
        outputPath,
        "-MockV2QualificationPath",
        path,
      ],
      { cwd: process.cwd(), encoding: "utf8", shell: false },
    );
  };
  const qualificationLine = (report) =>
    `PERSISTENCE_QUALIFICATION ${JSON.stringify(report)}`;
  assert.notEqual(
    (await rawQualificationResult(
      "missing-suite",
      `${qualificationLine(qualificationReports[0])}\n`,
    )).status,
    0,
    "qualification wrapper must reject a missing suite",
  );
  assert.notEqual(
    (await rawQualificationResult(
      "duplicate-suite",
      `${qualificationLine(qualificationReports[0])}\n${qualificationLine(qualificationReports[0])}\n${qualificationLine(qualificationReports[1])}\n`,
    )).status,
    0,
    "qualification wrapper must reject a duplicate suite",
  );
  assert.notEqual(
    (await rawQualificationResult(
      "damaged-json",
      `PERSISTENCE_QUALIFICATION {not-json}\n${qualificationLine(qualificationReports[1])}\n`,
    )).status,
    0,
    "qualification wrapper must reject damaged report JSON",
  );

  const invalidReports = async (name, mutate) => {
    const reports = structuredClone(qualificationReports);
    mutate(reports);
    const path = join(root, `${name}.txt`);
    await writeFile(
      path,
      `${reports
        .map((report) => `PERSISTENCE_QUALIFICATION ${JSON.stringify(report)}`)
        .join("\n")}\n`,
      "utf8",
    );
    return spawnSync(
      "powershell",
      [
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        "scripts/run-persistence-performance-qualification.ps1",
        "-BaselinePath",
        baselinePath,
        "-OutputPath",
        outputPath,
        "-MockV2QualificationPath",
        path,
      ],
      { cwd: process.cwd(), encoding: "utf8", shell: false },
    );
  };

  assert.notEqual(
    (await invalidReports("debug-build", (reports) => {
      reports[0].environment.debugAssertions = true;
    })).status,
    0,
    "qualification wrapper must reject debug assertions in V2 evidence",
  );
  assert.notEqual(
    (await invalidReports("unlocked-build", (reports) => {
      reports[0].provenance.build.locked = false;
    })).status,
    0,
    "qualification wrapper must reject V2 evidence without cargo --locked provenance",
  );
  assert.notEqual(
    (await invalidReports("wrong-cardinality", (reports) => {
      reports[0].workloads.requestLogs.rows = 499;
    })).status,
    0,
    "qualification wrapper must reject a V2 workload that is not 500/200",
  );
  assert.notEqual(
    (await invalidReports("memory-over-limit", (reports) => {
      reports[0].memory.peakPrivateUsageDeltaBytes = 268435457;
    })).status,
    0,
    "qualification wrapper must reject a PrivateUsage delta above 256 MiB",
  );
  assert.notEqual(
    (await invalidReports("queue-not-drained", (reports) => {
      reports[0].queues.writeCoordinator.currentDepth = 1;
    })).status,
    0,
    "qualification wrapper must reject a non-drained write queue",
  );
  assert.notEqual(
    (await invalidReports("queue-nonterminal", (reports) => {
      reports[0].queues.writeCoordinator.committedWrites = 40;
    })).status,
    0,
    "qualification wrapper must reject nonterminal write coordinator accounting",
  );
  assert.notEqual(
    (await invalidReports("zero-sample", (reports) => {
      reports[0].metrics.hotRequestLogs.samplesNs[0] = 0;
    })).status,
    0,
    "qualification wrapper must reject non-positive raw samples",
  );
  assert.notEqual(
    (await invalidReports("measurement-order", (reports) => {
      reports[0].provenance.measurementStartedAtUtc = "2026-07-20T23:00:00.0000000Z";
    })).status,
    0,
    "qualification wrapper must reject V2 measurements that precede the reconstructed V1 baseline",
  );

  const mismatchedMachineBaseline = structuredClone(baseline);
  mismatchedMachineBaseline.environment.cpuModel = "other CPU";
  await writeFile(baselinePath, `${JSON.stringify(mismatchedMachineBaseline)}\n`, "utf8");
  const mismatchedMachineResult = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "scripts/run-persistence-performance-qualification.ps1",
      "-BaselinePath",
      baselinePath,
      "-OutputPath",
      outputPath,
      "-MockV2QualificationPath",
      v2Path,
    ],
    { cwd: process.cwd(), encoding: "utf8", shell: false },
  );
  assert.notEqual(
    mismatchedMachineResult.status,
    0,
    "qualification wrapper must reject a V1/V2 machine mismatch",
  );
  await writeFile(baselinePath, `${JSON.stringify(baseline)}\n`, "utf8");

  assert.notEqual(
    (await invalidReports("v2-regressed", (reports) => {
      reports.find((report) => report.suite === "standard").metrics.hotRequestLogs.samplesNs =
        Array.from({ length: 40 }, () => 10_000);
    })).status,
    0,
    "qualification wrapper must reject a V2 p95 above the reconstructed V1 baseline by more than 10%",
  );

  for (const field of [
    "cpuModel",
    "logicalProcessors",
    "installedMemoryBytes",
    "windowsCaption",
    "windowsVersion",
    "windowsBuild",
    "activePowerScheme",
    "gitHead",
    "worktreeDirty",
    "worktreeStatusSha256",
    "antivirusProducts",
    "defenderRealTimeProtection",
    "windowsSearchService",
  ]) {
    assert.ok(
      Object.hasOwn(evidence.environment, field),
      `controlled environment must include ${field}`,
    );
  }

  assert.match(evidence.inputHashes.baselineSha256, /^[a-f0-9]{64}$/);
  assert.match(evidence.inputHashes.v2QualificationSha256, /^[a-f0-9]{64}$/);
  assert.match(evidence.outputPayloadSha256, /^[a-f0-9]{64}$/);

  const { build: _build, environment: _environment, ...uncontrolledBaseline } = baseline;
  await writeFile(baselinePath, `${JSON.stringify(uncontrolledBaseline)}\n`, "utf8");
  const uncontrolledResult = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "scripts/run-persistence-performance-qualification.ps1",
      "-BaselinePath",
      baselinePath,
      "-OutputPath",
      outputPath,
      "-MockV2QualificationPath",
      v2Path,
    ],
    { cwd: process.cwd(), encoding: "utf8", shell: false },
  );
  assert.notEqual(
    uncontrolledResult.status,
    0,
    "qualification wrapper must reject a baseline without release-build and controlled-environment provenance",
  );

  const incompleteSamplesBaseline = structuredClone(baseline);
  incompleteSamplesBaseline.metrics.hotRequestLogs.samplesNs = [100, 110, 120];
  await writeFile(baselinePath, `${JSON.stringify(incompleteSamplesBaseline)}\n`, "utf8");
  const incompleteSamplesResult = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "scripts/run-persistence-performance-qualification.ps1",
      "-BaselinePath",
      baselinePath,
      "-OutputPath",
      outputPath,
      "-MockV2QualificationPath",
      v2Path,
    ],
    { cwd: process.cwd(), encoding: "utf8", shell: false },
  );
  assert.notEqual(
    incompleteSamplesResult.status,
    0,
    "qualification wrapper must reject a baseline that drops retained raw samples",
  );

  const { workloads: _workloads, ...unnormalizedBaseline } = baseline;
  await writeFile(baselinePath, `${JSON.stringify(unnormalizedBaseline)}\n`, "utf8");
  const unnormalizedResult = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "scripts/run-persistence-performance-qualification.ps1",
      "-BaselinePath",
      baselinePath,
      "-OutputPath",
      outputPath,
      "-MockV2QualificationPath",
      v2Path,
    ],
    { cwd: process.cwd(), encoding: "utf8", shell: false },
  );
  assert.notEqual(
    unnormalizedResult.status,
    0,
    "qualification wrapper must reject a baseline without the reviewed 500/201-to-200/no-migration workload contract",
  );

  const { standardFixture: _standardFixture, ...nonstandardBaseline } = baseline;
  await writeFile(baselinePath, `${JSON.stringify(nonstandardBaseline)}\n`, "utf8");
  const nonstandardResult = spawnSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      "scripts/run-persistence-performance-qualification.ps1",
      "-BaselinePath",
      baselinePath,
      "-OutputPath",
      outputPath,
      "-MockV2QualificationPath",
      v2Path,
    ],
    { cwd: process.cwd(), encoding: "utf8", shell: false },
  );
  assert.notEqual(
    nonstandardResult.status,
    0,
    "qualification wrapper must reject a baseline without the 100/1,000/10,000/100,000 fixture contract",
  );
} finally {
  await rm(root, { recursive: true, force: true });
}
