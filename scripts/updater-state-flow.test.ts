import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import test from "node:test";

const modulePath = new URL("../src/features/updater/updateState.ts", import.meta.url);

test("updater state module exists", () => {
  assert.ok(existsSync(modulePath), "updateState.ts must exist");
});

test("confirmed update advances through download, cleanup, and install", async () => {
  const { initialUpdaterState, reduceUpdaterState } = await import(modulePath.href);
  const available = reduceUpdaterState(initialUpdaterState, {
    type: "UPDATE_AVAILABLE",
    currentVersion: "0.1.0",
    version: "0.1.1",
    notes: "Fixes",
  });
  const downloading = reduceUpdaterState(available, { type: "DOWNLOAD_STARTED" });
  const progressed = reduceUpdaterState(downloading, {
    type: "DOWNLOAD_PROGRESS",
    downloadedBytes: 50,
    totalBytes: 100,
  });
  const cleaning = reduceUpdaterState(progressed, { type: "CLEANUP_STARTED" });
  const installing = reduceUpdaterState(cleaning, { type: "INSTALL_STARTED" });

  assert.equal(available.phase, "available");
  assert.equal(progressed.downloadedBytes, 50);
  assert.equal(progressed.totalBytes, 100);
  assert.equal(cleaning.phase, "cleaning");
  assert.equal(installing.phase, "installing");
});

test("failed check remains retryable", async () => {
  const { initialUpdaterState, reduceUpdaterState } = await import(modulePath.href);
  const checking = reduceUpdaterState(initialUpdaterState, { type: "CHECK_STARTED" });
  const failed = reduceUpdaterState(checking, { type: "FAILED", message: "offline" });
  const retried = reduceUpdaterState(failed, { type: "CHECK_STARTED" });

  assert.equal(failed.phase, "failed");
  assert.equal(failed.error, "offline");
  assert.equal(retried.phase, "checking");
  assert.equal(retried.error, null);
});

test("up-to-date result clears stale available update details", async () => {
  const { initialUpdaterState, reduceUpdaterState } = await import(modulePath.href);
  const available = reduceUpdaterState(initialUpdaterState, {
    type: "UPDATE_AVAILABLE",
    currentVersion: "0.1.0",
    version: "0.1.1",
    notes: "Fixes",
  });
  const current = reduceUpdaterState(available, {
    type: "UP_TO_DATE",
    currentVersion: "0.1.0",
    checkedAt: "2026-07-11T00:00:00.000Z",
  });

  assert.equal(current.phase, "idle");
  assert.equal(current.version, null);
  assert.equal(current.notes, null);
});
