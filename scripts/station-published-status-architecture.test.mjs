import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const rootIndex = args.indexOf("--root");
const root = path.resolve(rootIndex >= 0 ? args[rootIndex + 1] : process.cwd());
const fixturesOnly = args.includes("--fixtures-only");
const failures = [];
const fixtureRelativeRoot = "src-tauri/src/services/collectors/drivers/sub2api/fixtures/published_status";

run("published-status fixture contract", checkFixtures);
if (!fixturesOnly) run("published-status production architecture", checkProductionArchitecture);

if (failures.length > 0) {
  console.error(failures.map((failure) => `- ${failure}`).join("\n"));
  process.exit(1);
}

console.log(
  fixturesOnly
    ? "station published-status fixture contract passed"
    : "station published-status architecture gate passed",
);

function run(label, callback) {
  try {
    callback();
  } catch (error) {
    failures.push(`${label}: ${error instanceof Error ? error.message : String(error)}`);
  }
}

function checkFixtures() {
  const fixtureRoot = resolve(fixtureRelativeRoot);
  assert.ok(existsSync(fixtureRoot), `${fixtureRelativeRoot} must exist`);

  const manifest = readJson(`${fixtureRelativeRoot}/manifest.json`);
  assert.equal(manifest.schemaVersion, 1, "fixture manifest schemaVersion must be 1");
  assert.equal(manifest.fixtureKind, "sub2api-published-status", "fixture manifest kind drifted");
  assert.equal(manifest.request.method, "GET", "published status must use a GET list request");
  assert.equal(manifest.request.path, "/api/v1/channel-monitors", "published status API path drifted");
  assert.equal(manifest.request.requestsPerCollection, 1, "published status must use one list request per collection");
  assert.equal(manifest.request.responseBodyLimitBytes, 4 * 1024 * 1024, "response body limit drifted");
  assert.deepEqual(manifest.limits, {
    maxMonitorsPerBatch: 512,
    maxTimelineItemsPerMonitor: 240,
    maxRetainedSamplesPerMonitorModel: 60,
    maxSourceStatusBytes: 64,
    maxSafeMessageBytes: 512,
    maxMonitorIdBytes: 128,
    maxMonitorNameBytes: 128,
    maxModelNameBytes: 128,
    maxProviderNameBytes: 128,
    maxGroupNameBytes: 128,
  }, "published-status limits must remain frozen in one fixture manifest");
  assert.deepEqual(manifest.statusMapping, {
    healthy: "available",
    degraded: "degraded",
    failed: "unavailable",
    unrecognizedOrNull: "unknown",
  }, "status mapping must remain explicit and fail closed");
  assert.ok(Array.isArray(manifest.cases), "fixture manifest cases must be an array");

  const requiredPayloads = [
    "complete-60.json",
    "normalization-boundaries.json",
    "empty-success.json",
    "unknown-and-nullable.json",
    "partial-malformed-item.json",
    "malformed-envelope.json",
    "http-errors.json",
  ];
  assert.deepEqual(
    manifest.cases.map((item) => item.payload).sort(),
    requiredPayloads.slice().sort(),
    "fixture manifest must list every required published-status payload exactly once",
  );
  assert.deepEqual(
    readdirSync(fixtureRoot).filter((file) => file.endsWith(".json")).sort(),
    ["manifest.json", ...requiredPayloads].sort(),
    "published-status fixture directory must not contain unmanifested JSON",
  );

  const payloads = new Map(requiredPayloads.map((file) => [file, readJson(`${fixtureRelativeRoot}/${file}`)]));
  assertNoSensitiveMaterial(manifest, "manifest");
  for (const [file, payload] of payloads) assertNoSensitiveMaterial(payload, file);

  const completeTimeline = payloads.get("complete-60.json").data.items[0].timeline;
  assert.equal(completeTimeline.length, 60, "complete fixture must contain exactly 60 samples");
  assert.equal(new Set(completeTimeline.map((sample) => sample.checked_at)).size, 60, "complete fixture samples must have unique timestamps");
  assert.ok(completeTimeline.every((sample) => sample.model === "gpt-4o-mini"), "complete fixture must preserve its primary model");

  const boundaryItems = payloads.get("normalization-boundaries.json").data.items;
  const underLimit = boundaryItems.find((item) => item.id === "monitor-under-limit").timeline;
  const overLimit = boundaryItems.find((item) => item.id === "monitor-over-limit").timeline;
  const duplicateTimeline = boundaryItems.find((item) => item.id === "monitor-unordered-duplicates").timeline;
  assert.ok(underLimit.length < 60, "normalization fixture must cover a timeline below retention");
  assert.ok(overLimit.length > 60, "normalization fixture must cover a timeline above retention");
  assert.notDeepEqual(
    duplicateTimeline.map((sample) => sample.checked_at),
    duplicateTimeline.map((sample) => sample.checked_at).slice().sort(),
    "normalization fixture must not assume upstream timeline ordering",
  );
  assert.ok(
    duplicateTimeline.filter((sample) => sample.checked_at === "2026-08-15T03:02:00Z").length === 2,
    "normalization fixture must include an identical duplicate timestamp",
  );
  assert.ok(
    new Set(
      duplicateTimeline
        .filter((sample) => sample.checked_at === "2026-08-15T03:01:00Z")
        .map((sample) => JSON.stringify(sample)),
    ).size === 2,
    "normalization fixture must include a conflicting duplicate timestamp",
  );

  assert.deepEqual(payloads.get("empty-success.json").data.items, [], "empty success must be an explicit legal item list");
  const nullableMonitor = payloads.get("unknown-and-nullable.json").data.items[0];
  assert.equal(nullableMonitor.primary_status, "new-status-not-yet-mapped", "unknown fixture must not use a known status alias");
  assert.equal(nullableMonitor.primary_latency_ms, null, "unknown fixture must cover nullable latency");
  assert.equal(nullableMonitor.primary_ping_latency_ms, null, "unknown fixture must cover nullable ping latency");
  assert.equal(nullableMonitor.timeline[0].status, null, "unknown fixture must cover null timeline status");

  const partialItems = payloads.get("partial-malformed-item.json").data.items;
  assert.ok(partialItems.some((item) => typeof item !== "object" || item === null), "partial fixture must contain a malformed list item");
  assert.ok(
    partialItems.some((item) => item?.id === "monitor-valid-among-malformed"),
    "partial fixture must retain one valid monitor beside malformed input",
  );
  assert.ok(!("items" in payloads.get("malformed-envelope.json").data), "malformed envelope must not be treated as an empty list");

  const errors = payloads.get("http-errors.json").responses;
  assert.deepEqual(errors.map((item) => item.httpStatus), [401, 403, 404, 429, 500], "HTTP error fixture coverage drifted");
  assert.deepEqual(
    errors.map((item) => item.expectedSourceState),
    ["authorization_required", "authorization_required", "unsupported", "failed", "failed"],
    "HTTP error source-state classification drifted",
  );
}

function checkProductionArchitecture() {
  const requiredFiles = [
    "src-tauri/src/models/station_published_status.rs",
    "src-tauri/src/services/collectors/drivers/sub2api/published_status.rs",
    "src-tauri/src/persistence/stores/station_published_status_store.rs",
    "src-tauri/src/application/queries/station_published_status.rs",
    "src-tauri/src/ipc/dto/station_published_status.rs",
    "src-tauri/src/commands/station_published_status.rs",
    "src/lib/types/stationPublishedStatus.ts",
    "src/lib/api/stationPublishedStatus.ts",
    "src/features/stations/components/StationPublishedStatusSection.tsx",
  ];
  for (const relative of requiredFiles) assert.ok(existsSync(resolve(relative)), `${relative} must exist`);

  const migrationDirectory = resolve("src-tauri/src/persistence/migrations");
  const migrations = existsSync(migrationDirectory)
    ? readdirSync(migrationDirectory).filter((file) => /^\d+_station_published_status\.sql$/u.test(file))
    : [];
  assert.equal(migrations.length, 1, "exactly one published-status migration must be present");

  const domainSource = readSource("src-tauri/src/models/station_published_status.rs");
  assert.match(domainSource, /PublishedStatusBatch/u, "domain model must own PublishedStatusBatch");
  assertNoMatch(
    domainSource,
    /\b(?:tauri|sqlx|reqwest|tokio|SecretManager|secret_manager|services::monitoring|application::monitoring|persistence::stores::monitoring)\b/u,
    "published-status domain model must not depend on infrastructure or monitoring",
  );

  const collectorContract = readSource("src-tauri/src/services/collectors/contract.rs");
  const collectorOutput = readSource("src-tauri/src/services/collectors/output.rs");
  assert.match(collectorContract, /enum\s+CollectorTaskKind[\s\S]*\bPublishedStatus\b/u, "collector task kind must include PublishedStatus");
  assert.match(collectorOutput, /enum\s+CollectorTask[\s\S]*\bPublishedStatus\b/u, "collector task must include PublishedStatus");

  const sub2apiDriver = productionSource("src-tauri/src/services/collectors/drivers/sub2api/mod.rs");
  const newApiDriver = readSource("src-tauri/src/services/collectors/drivers/newapi/mod.rs");
  const publishedStatusDriver = productionSource("src-tauri/src/services/collectors/drivers/sub2api/published_status.rs");
  assert.match(sub2apiDriver, /SUPPORTED_COLLECTOR_TASKS[\s\S]*\bPublishedStatus\b/u, "Sub2API must declare PublishedStatus support");
  assert.match(sub2apiDriver, /FULL_COLLECTOR_TASKS[\s\S]*\bPublishedStatus\b/u, "Sub2API Full collection must include PublishedStatus");
  assert.doesNotMatch(
    newApiDriver,
    /pub const SUPPORTED_COLLECTOR_TASKS: &\[CollectorTaskKind\] = \[[^\]]*\bPublishedStatus\b/u,
    "NewAPI must not advertise the unsupported PublishedStatus task",
  );
  assert.doesNotMatch(
    newApiDriver,
    /pub const FULL_COLLECTOR_TASKS: &\[CollectorTaskKind\] = \[[^\]]*\bPublishedStatus\b/u,
    "NewAPI Full collection must not include PublishedStatus",
  );
  assert.match(sub2apiDriver, /\/api\/v1\/channel-monitors\b/u, "Sub2API driver must use the official list endpoint");
  assertNoMatch(
    sub2apiDriver,
    /\/api\/v1\/channel-monitors\//u,
    "Sub2API published-status driver must not issue per-monitor detail requests",
  );
  assertNoMatch(
    sub2apiDriver,
    /\/monitor(?:[/'"`]|$)/u,
    "Sub2API published-status driver must not scrape a monitor HTML page",
  );
  assertNoMatch(
    publishedStatusDriver,
    /availability_7d/u,
    "published-status parser must not collect upstream seven-day availability",
  );

  const allPublishedStatusRust = listFiles(resolve("src-tauri/src"), (file) =>
    file.endsWith(".rs") && /(?:station_published_status|published_status)/u.test(file),
  );
  assert.ok(allPublishedStatusRust.length >= 6, "published-status production source set is incomplete");
  for (const file of allPublishedStatusRust) {
    const source = readFileSync(file, "utf8");
    assertNoMatch(
      source,
      /\b(?:channel_monitor(?:s|_[a-z_]+)?|station_key_health|application::monitoring|services::monitoring|persistence::stores::monitoring|models::monitoring)\b/u,
      `${relativePath(file)} must not depend on active monitoring or routing health facts`,
    );
  }

  const storeSource = readSource("src-tauri/src/persistence/stores/station_published_status_store.rs");
  assert.match(storeSource, /station_published_status_sources/u, "published-status store must own its source facts");
  assert.match(storeSource, /station_published_monitors/u, "published-status store must own its monitor facts");
  assert.match(storeSource, /station_published_monitor_samples/u, "published-status store must own its sample facts");
  assert.match(
    storeSource,
    /availability_7d_percent\s*=\s*NULL/u,
    "published-status writes must clear the legacy seven-day availability column",
  );
  assertNoMatch(storeSource, /\b(?:channel_monitor|station_key_health)\b/u, "published-status store must not write active monitoring or key-health tables");

  const registrySource = readSource("src-tauri/src/ipc/registry.rs");
  assert.match(registrySource, /station_published_status/u, "IPC registry must expose the dedicated workspace command");
  const sectionSource = readSource("src/features/stations/components/StationPublishedStatusSection.tsx");
  const apiSource = readSource("src/lib/api/stationPublishedStatus.ts");
  const typeSource = readSource("src/lib/types/stationPublishedStatus.ts");
  assertNoMatch(sectionSource, /(?:collector_snapshot|collectorSnapshots|raw_json|rawJson)/u, "station detail UI must render the dedicated read model, not collector snapshots");
  assertNoMatch(apiSource, /(?:collector_snapshot|collectorSnapshots|raw_json|rawJson)/u, "published-status API must not expose collector snapshots or raw upstream JSON");
  assert.match(typeSource, /\brecentAvailabilityPercent\b/u, "published-status DTO must expose recent sample availability");
  assertNoMatch(typeSource, /\bavailability7dPercent\b/u, "published-status DTO must not expose upstream seven-day availability");
}

function assertNoSensitiveMaterial(value, label) {
  const forbiddenKey = /(?:api[_-]?key|authorization|cookie|(?:access|refresh)?[_-]?token|password|secret)/iu;
  const forbiddenValue = /(?:\bsk-[A-Za-z0-9_-]{8,}|\bBearer\s+[A-Za-z0-9._-]{8,}|fake-token-not-a-secret)/iu;
  walk(value, label, (candidate, candidateLabel) => {
    if (candidate && typeof candidate === "object" && !Array.isArray(candidate)) {
      for (const key of Object.keys(candidate)) {
        assert.ok(!forbiddenKey.test(key), `${candidateLabel}.${key} looks like sensitive material`);
      }
    }
    if (typeof candidate === "string") {
      assert.ok(!forbiddenValue.test(candidate), `${candidateLabel} contains a secret-shaped value`);
    }
  });
}

function walk(value, label, visit) {
  visit(value, label);
  if (Array.isArray(value)) {
    value.forEach((child, index) => walk(child, `${label}[${index}]`, visit));
  } else if (value && typeof value === "object") {
    Object.entries(value).forEach(([key, child]) => walk(child, `${label}.${key}`, visit));
  }
}

function assertNoMatch(source, pattern, message) {
  assert.doesNotMatch(source, pattern, message);
}

function readJson(relative) {
  return JSON.parse(readSource(relative));
}

function readSource(relative) {
  const absolute = resolve(relative);
  assert.ok(existsSync(absolute), `${relative} must exist`);
  return readFileSync(absolute, "utf8");
}

function productionSource(relative) {
  return readSource(relative).split("#[cfg(test)]", 1)[0];
}

function resolve(relative) {
  return path.join(root, ...relative.split("/"));
}

function relativePath(file) {
  return path.relative(root, file).replaceAll("\\", "/");
}

function listFiles(directory, include) {
  if (!existsSync(directory)) return [];
  const files = [];
  const pending = [directory];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const file = path.join(current, entry.name);
      if (entry.isDirectory()) pending.push(file);
      else if (entry.isFile() && include(file)) files.push(file);
    }
  }
  return files.sort();
}
