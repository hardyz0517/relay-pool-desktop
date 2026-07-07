import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const sourceFiles = [];
await collectSourceFiles(path.join(root, "src", "features"), sourceFiles);

const localReadErrorDefinitions = [];
const localFormatRateDefinitions = [];
const localChannelToTimeDefinitions = [];
const localTrimmedDecimalDefinitions = [];
for (const absolutePath of sourceFiles) {
  const source = await readFile(absolutePath, "utf8");
  const relativePath = normalizePath(path.relative(root, absolutePath));
  if (/function\s+readError\s*\(/.test(source)) {
    localReadErrorDefinitions.push(relativePath);
  }
  if (
    /^src\/features\/(?:logs\/LogsPage|routing\/RoutingPage)\.tsx$/.test(relativePath) &&
    /function\s+formatRate\s*\(/.test(source)
  ) {
    localFormatRateDefinitions.push(relativePath);
  }
  if (
    /^src\/features\/channels\/(?:ChannelStatusTab|channelMonitorViewModel|channelStatusViewModel)\.tsx?$/.test(
      relativePath,
    ) &&
    /function\s+toTime\s*\([^)]*\)\s*\{\s*const\s+numeric\s*=\s*Number\(value\);/.test(source)
  ) {
    localChannelToTimeDefinitions.push(relativePath);
  }
  if (
    /^src\/features\/(?:pricing\/PricingPage|stations\/stationDetailViewModels)\.tsx?$/.test(relativePath) &&
    /\.toFixed\([^)]*\)\.replace\(\/0\+\$\/,\s*""\)\.replace\(\/\\\.\$\/,\s*""\)/.test(source)
  ) {
    localTrimmedDecimalDefinitions.push(relativePath);
  }
}

assert.deepEqual(
  localReadErrorDefinitions,
  [],
  `feature pages should import readError from src/lib/errors.ts instead of redefining it:\n${localReadErrorDefinitions.join("\n")}`,
);

const sharedErrorsSource = await readFile(path.join(root, "src", "lib", "errors.ts"), "utf8");
assert.ok(
  sharedErrorsSource.includes("export function readError(error: unknown)"),
  "shared readError helper should remain exported from src/lib/errors.ts",
);

assert.deepEqual(
  localFormatRateDefinitions,
  [],
  `logs and routing pages should import formatRate from src/lib/formatters.ts instead of redefining it:\n${localFormatRateDefinitions.join("\n")}`,
);

const sharedFormattersSource = await readFile(path.join(root, "src", "lib", "formatters.ts"), "utf8");
assert.ok(
  sharedFormattersSource.includes("export function formatRate(value: number | null | undefined"),
  "shared formatRate helper should remain exported from src/lib/formatters.ts",
);
assert.ok(
  sharedFormattersSource.includes("export function formatTrimmedDecimal(value: number, fractionDigits: number)"),
  "shared formatTrimmedDecimal helper should remain exported from src/lib/formatters.ts",
);

assert.deepEqual(
  localTrimmedDecimalDefinitions,
  [],
  `pricing and station detail should use formatTrimmedDecimal from src/lib/formatters.ts instead of redefining trailing-zero trimming:\n${localTrimmedDecimalDefinitions.join("\n")}`,
);

assert.deepEqual(
  localChannelToTimeDefinitions,
  [],
  `channel views should import timestamp parsing from src/lib/time.ts instead of redefining it:\n${localChannelToTimeDefinitions.join("\n")}`,
);

const sharedTimeSource = await readFile(path.join(root, "src", "lib", "time.ts"), "utf8");
assert.ok(
  sharedTimeSource.includes("export function parseTimestampLikeDate(value: string)"),
  "shared parseTimestampLikeDate helper should remain exported from src/lib/time.ts",
);
assert.ok(
  sharedTimeSource.includes("export function toTimestampMillis(value: string)"),
  "shared toTimestampMillis helper should remain exported from src/lib/time.ts",
);

async function collectSourceFiles(directory, files) {
  const entries = await readdir(directory, { withFileTypes: true });
  for (const entry of entries) {
    const absolutePath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      await collectSourceFiles(absolutePath, files);
      continue;
    }
    if (/\.(ts|tsx)$/.test(entry.name)) {
      files.push(absolutePath);
    }
  }
}

function normalizePath(value) {
  return value.split(path.sep).join("/");
}
