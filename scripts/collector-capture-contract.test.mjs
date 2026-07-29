/* global console */

import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const commands = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const registry = await readFile("src-tauri/src/ipc/registry.rs", "utf8");
const collectorApi = await readFile("src/lib/api/collector.ts", "utf8");

for (const command of [
  "start_capture_session",
  "get_capture_session_status",
  "finish_capture_session",
  "finish_web_authorization_session",
  "clear_capture_session",
  "close_capture_session",
]) {
  const camel = command.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
  assert.ok(
    collectorApi.includes(`${camel}Generated({ stationId })`) ||
      collectorApi.includes(`${camel}Generated({ stationId }).catch`),
    `collector API should route ${command} through the generated wrapper`,
  );
  assert.ok(
    !collectorApi.includes(`invoke<CaptureSessionStatus>("${command}"`) &&
      !collectorApi.includes(`invoke<CollectorRunResult>("${command}"`),
    `collector API should not invoke ${command} directly`,
  );
}

assert.ok(
  commands.includes("CapturedHttpEventInputDto::parse(input)?.into_domain()"),
  "record_capture_event should validate through the strict capture DTO before domain conversion",
);
assert.ok(
  registry.includes('"record_capture_event" => migrated_mutation(') &&
    registry.includes('"CapturedHttpEventInputDto"') &&
    registry.includes('"CaptureSessionStatusDto"'),
  "registry should declare record_capture_event strict input and output DTOs",
);

for (const featureFile of await listSourceFiles("src/features")) {
  const source = await readFile(featureFile, "utf8");
  for (const command of [
    "start_capture_session",
    "get_capture_session_status",
    "finish_capture_session",
    "finish_web_authorization_session",
    "clear_capture_session",
    "close_capture_session",
  ]) {
    assert.ok(
      !source.includes(`"${command}"`) && !source.includes(`'${command}'`),
      `${featureFile} should call the collector API instead of invoking ${command} directly`,
    );
  }
}

console.log("collector capture contract passed");

async function listSourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        return listSourceFiles(entryPath);
      }
      return /\.[cm]?[jt]sx?$/.test(entry.name) ? [entryPath] : [];
    }),
  );
  return files.flat();
}
