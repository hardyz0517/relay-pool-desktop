import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const scanRoots = ["src", "src-tauri/src"];
const compatibilityFields = [
  "balanceRaw",
  "balanceCny",
  "lastPricingFetchedAt",
  "groupIdHash",
  "groupName",
  "rateMultiplier",
  "rateSource",
  "rateCollectedAt",
  "group_id_hash",
  "group_name",
  "rate_multiplier",
  "rate_source",
  "rate_collected_at",
  "balance_raw",
  "balance_cny",
  "last_pricing_fetched_at",
];

const allowedPathPatterns = [
  /^src[\\/]lib[\\/]types[\\/]/,
  /^src[\\/]lib[\\/]api[\\/]/,
  /^src[\\/]lib[\\/]mock[\\/]/,
  /^src[\\/]lib[\\/]queries[\\/]/,
  /^src[\\/]lib[\\/]projections[\\/]/,
  /^src[\\/]features[\\/]changes[\\/]changeEventViewModels\.ts$/,
  /^src[\\/]features[\\/]channels[\\/]ChannelMonitorForm\.tsx$/,
  /^src[\\/]features[\\/]channels[\\/]ChannelStatusTab\.tsx$/,
  /^src[\\/]features[\\/]collectors[\\/]CollectorsPage\.tsx$/,
  /^src[\\/]features[\\/]logs[\\/]LogsPage\.tsx$/,
  /^src[\\/]features[\\/]pricing[\\/]pricingComparisonViewModel\.ts$/,
  /^src[\\/]features[\\/]pricing[\\/]PricingPage\.tsx$/,
  /^src[\\/]features[\\/]routing[\\/]RoutingPage\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]AddProviderPage\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]groupOptionViewModels\.ts$/,
  /^src[\\/]features[\\/]stations[\\/]stationDetailViewModels\.ts$/,
  /^src[\\/]features[\\/]stations[\\/]stationAssetViewModels\.ts$/,
  /^src[\\/]features[\\/]stations[\\/]StationsPage\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]components[\\/]CreateRemoteKeyDialog\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]components[\\/]RemoteKeyDiscoveryList\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]components[\\/]StationDetailContent\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]components[\\/]StationDetailPanel\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]components[\\/]StationGroupRowsEditor\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]components[\\/]StationKeyRowsEditor\.tsx$/,
  /^src[\\/]features[\\/]stations[\\/]components[\\/]StationListItem\.tsx$/,
  /^src[\\/]features[\\/]key-pool[\\/]/,
  /^src-tauri[\\/]src[\\/]models[\\/]/,
  /^src-tauri[\\/]src[\\/]commands[\\/]mod\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]capture[\\/]mod\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]change_events\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]channel_monitors[\\/]mod\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]database\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]pricing[\\/]mod\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]proxy[\\/]router\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]proxy[\\/]routing_policy\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]proxy[\\/]runtime\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]remote_keys\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]shared_capabilities\.rs$/,
  /^src-tauri[\\/]src[\\/]services[\\/]collectors[\\/]/,
];

const sourceFiles = [];
for (const scanRoot of scanRoots) {
  await collectSourceFiles(path.join(root, scanRoot), sourceFiles);
}

const violations = [];
for (const absolutePath of sourceFiles) {
  const relativePath = normalizePath(path.relative(root, absolutePath));
  if (allowedPathPatterns.some((pattern) => pattern.test(relativePath))) {
    continue;
  }

  const source = await readFile(absolutePath, "utf8");
  for (const field of compatibilityFields) {
    const pattern = new RegExp(`(?<![A-Za-z0-9_])${escapeRegExp(field)}(?![A-Za-z0-9_])`, "g");
    if (pattern.test(source)) {
      violations.push(`${relativePath}: ${field}`);
    }
  }
}

assert.deepEqual(
  violations,
  [],
  `compatibility fields must be read through approved legacy/projection/query boundaries:\n${violations.join("\n")}`,
);

async function collectSourceFiles(directory, files) {
  const entries = await readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    const absolutePath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "node_modules" || entry.name === "target" || entry.name === "dist") {
        continue;
      }
      await collectSourceFiles(absolutePath, files);
      continue;
    }
    if (/\.(ts|tsx|rs)$/.test(entry.name)) {
      files.push(absolutePath);
    }
  }
}

function normalizePath(value) {
  return value.split(path.sep).join("/");
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
