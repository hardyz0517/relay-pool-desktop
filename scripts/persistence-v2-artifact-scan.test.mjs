import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const scanner = "scripts/verify-persistence-v2-artifacts.mjs";

function run(args) {
  return spawnSync("node", [scanner, ...args], { encoding: "utf8", shell: false });
}

const clean = await mkdtemp(join(tmpdir(), "relay-pool-artifact-clean-"));
const dirty = await mkdtemp(join(tmpdir(), "relay-pool-artifact-dirty-"));
const tracked = await mkdtemp(join(tmpdir(), "relay-pool-artifact-index-"));
const sqlite = join(dirty, "unsafe-fixture.sqlite3");

try {
  await writeFile(join(clean, "latest.json"), '{"version":"0.3.2"}\n');
  assert.equal(run(["--artifact", clean]).status, 0);

  const canary = ["RPD", "CANARY", "TOKEN", "0123456789"].join("_");
  await writeFile(join(dirty, "diagnostic.json"), JSON.stringify({ token: canary }));
  const canaryResult = run(["--artifact", dirty, "--canary", canary]);
  assert.equal(canaryResult.status, 1);
  assert.match(canaryResult.stderr, /seeded secret canary/);
  assert.doesNotMatch(canaryResult.stderr, new RegExp(canary));

  await rm(join(dirty, "diagnostic.json"));
  const localPath = join(homedir(), "RelayPool", "relay-pool-desktop.sqlite3");
  await writeFile(join(dirty, "upgrade-report.json"), JSON.stringify({ path: localPath }));
  const pathResult = run(["--artifact", dirty]);
  assert.equal(pathResult.status, 1);
  assert.match(pathResult.stderr, /absolute (?:Windows|Unix) user path/);
  assert.doesNotMatch(pathResult.stderr, new RegExp(homedir().replaceAll("\\", "\\\\"), "i"));

  await rm(join(dirty, "upgrade-report.json"));
  const python = [
    "import sqlite3, sys",
    "c=sqlite3.connect(sys.argv[1])",
    "c.execute('create table evidence(id text, api_key text, note text)')",
    "c.execute('insert into evidence values(?, ?, ?)', ('row-1', 'not-sanitized', 'safe'))",
    "c.execute('create table settings(key text, value text)')",
    "c.execute('insert into settings values(?, ?)', ('local_key', 'not-sanitized'))",
    "c.commit()",
    "c.close()",
  ].join("\n");
  const created = spawnSync("python", ["-c", python, sqlite], { encoding: "utf8", shell: false });
  assert.equal(created.status, 0, created.stderr);
  const sqliteResult = run(["--sqlite", sqlite]);
  assert.equal(sqliteResult.status, 1);
  assert.match(sqliteResult.stderr, /evidence\.api_key: non-empty sensitive column value/);
  assert.match(sqliteResult.stderr, /settings\.value: non-empty sensitive column value/);
  assert.doesNotMatch(sqliteResult.stderr, /not-sanitized/);

  await mkdir(join(dirty, "bundle"));
  await writeFile(join(dirty, "bundle", "leaked.db-journal"), "local state");
  const artifactResult = run(["--artifact", join(dirty, "bundle")]);
  assert.equal(artifactResult.status, 1);
  assert.match(artifactResult.stderr, /database, journal, backup, or log artifact is bundled/);

  const policy = join(tracked, "policy.json");
  await writeFile(policy, '{"version":1,"trackedDatabaseFixtures":[]}\n');
  await writeFile(join(tracked, "leaked.sqlite3-wal"), "tracked local state");
  assert.equal(spawnSync("git", ["init", "--quiet", tracked], { shell: false }).status, 0);
  assert.equal(
    spawnSync("git", ["-C", tracked, "add", "--", "leaked.sqlite3-wal"], { shell: false }).status,
    0,
  );
  const indexResult = run(["--repo-root", tracked, "--policy", policy, "--tracked"]);
  assert.equal(indexResult.status, 1);
  assert.match(indexResult.stderr, /database, journal, backup, or log artifact is tracked/);

  const symlinkBlob = spawnSync("git", ["-C", tracked, "hash-object", "-w", "--stdin"], {
    encoding: "utf8",
    input: "outside.txt",
    shell: false,
  });
  assert.equal(symlinkBlob.status, 0, symlinkBlob.stderr);
  assert.equal(
    spawnSync(
      "git",
      [
        "-C",
        tracked,
        "update-index",
        "--add",
        "--cacheinfo",
        `120000,${symlinkBlob.stdout.trim()},unsafe-link`,
      ],
      { shell: false },
    ).status,
    0,
  );
  const symlinkResult = run(["--repo-root", tracked, "--policy", policy, "--tracked"]);
  assert.equal(symlinkResult.status, 1);
  assert.match(symlinkResult.stderr, /symbolic links, submodules, and special index entries/);
  assert.equal(
    spawnSync("git", ["-C", tracked, "update-index", "--force-remove", "unsafe-link"], {
      shell: false,
    }).status,
    0,
  );

  const fixture = join(tracked, "fixture.sqlite3");
  const manifest = join(tracked, "fixture-manifest.json");
  await writeFile(fixture, "not a real database");
  await writeFile(manifest, JSON.stringify({ fixture_sha256: "0".repeat(64) }));
  await writeFile(
    policy,
    JSON.stringify({
      version: 1,
      trackedDatabaseFixtures: [
        { path: "fixture.sqlite3", manifest: "fixture-manifest.json", allowedSensitiveValues: [] },
      ],
    }),
  );
  assert.equal(
    spawnSync("git", ["-C", tracked, "add", "--", "fixture.sqlite3", "fixture-manifest.json"], {
      shell: false,
    }).status,
    0,
  );
  const hashResult = run(["--repo-root", tracked, "--policy", policy, "--tracked"]);
  assert.equal(hashResult.status, 1);
  assert.match(hashResult.stderr, /SHA-256 does not match fixture-manifest\.json/);

  const trackedResult = run(["--tracked"]);
  assert.equal(trackedResult.status, 0, trackedResult.stderr);
} finally {
  await rm(clean, { force: true, recursive: true });
  await rm(dirty, { force: true, recursive: true });
  await rm(tracked, { force: true, recursive: true });
}
