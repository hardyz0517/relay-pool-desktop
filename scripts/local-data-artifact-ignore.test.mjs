import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";

const localArtifacts = [
  "probe.db",
  "probe.db-wal",
  "probe.db-shm",
  "probe.db-journal",
  "probe.db.bak",
  "probe.db.backup",
  "probe.sqlite",
  "probe.sqlite-wal",
  "probe.sqlite-shm",
  "probe.sqlite-journal",
  "probe.sqlite.bak",
  "probe.sqlite.backup",
  "probe.sqlite3",
  "probe.sqlite3-wal",
  "probe.sqlite3-shm",
  "probe.sqlite3-journal",
  "probe.sqlite3.bak",
  "probe.sqlite3.backup",
  "probe.sqlite3.tmp",
  "probe.sqlite3.tmp-wal",
  "probe.sqlite3.tmp-shm",
  "probe.sqlite3.first-run.tmp",
  "probe.sqlite3.first-run.tmp-wal",
  "probe.sqlite3.first-run.tmp-shm",
  "probe.sqlite3.upgrade-attempt.tmp",
  "probe.sqlite3.upgrade-attempt.tmp-wal",
  "probe.sqlite3.upgrade-attempt.tmp-shm",
  "upgrade-report-2026-07-22.json",
  "diagnostic-report-2026-07-22.json",
];

for (const artifact of localArtifacts) {
  const result = spawnSync(
    "git",
    ["check-ignore", "--no-index", "--quiet", "--", artifact],
  );

  assert.equal(
    result.status,
    0,
    `${artifact} must remain ignored so local SQLite state cannot be committed`,
  );
}
