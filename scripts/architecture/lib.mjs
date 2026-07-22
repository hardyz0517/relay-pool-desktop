import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const rootArgumentIndex = process.argv.indexOf("--root");
export const repoRoot = rootArgumentIndex >= 0
  ? path.resolve(process.argv[rootArgumentIndex + 1] ?? "")
  : path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

export function fail(message) {
  throw new Error(message);
}

export function assert(condition, message) {
  if (!condition) fail(message);
}

export function readJson(relativePath, description = relativePath) {
  const absolutePath = path.resolve(repoRoot, relativePath);
  assert(fs.existsSync(absolutePath), `missing ${description}: ${relativePath}`);
  try {
    return JSON.parse(fs.readFileSync(absolutePath, "utf8"));
  } catch (error) {
    fail(`invalid ${description} ${relativePath}: ${error.message}`);
  }
}

export function readRequiredManifest(relativePath, requiredKeys) {
  const value = readJson(relativePath, "architecture manifest");
  assert(value && typeof value === "object" && !Array.isArray(value), `${relativePath} must be an object`);
  for (const key of requiredKeys) {
    assert(Object.hasOwn(value, key), `${relativePath} is missing required key '${key}'`);
  }
  return value;
}

export function normalizePath(value) {
  return value.replaceAll("\\", "/").replace(/^\.\//, "");
}

export function relativeToRoot(absolutePath) {
  return normalizePath(path.relative(repoRoot, absolutePath));
}

export function listFiles(root, predicate = () => true) {
  const result = [];
  if (!fs.existsSync(root)) return result;
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const absolute = path.join(root, entry.name);
    if (entry.isDirectory()) result.push(...listFiles(absolute, predicate));
    else if (entry.isFile() && predicate(absolute)) result.push(absolute);
  }
  return result.sort();
}

export function command(command, args, options = {}) {
  return execFileSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
    ...options,
  });
}

export function parseIsoDate(value, field) {
  assert(typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value), `${field} must be YYYY-MM-DD`);
  const date = new Date(`${value}T00:00:00Z`);
  assert(!Number.isNaN(date.valueOf()), `${field} is not a valid date`);
  return date;
}

export function authoritativeStage(manifest, context = "architecture manifest") {
  const currentStage = manifest?.current_stage;
  assert(Number.isInteger(currentStage) && currentStage >= 0, `${context}.current_stage must be a non-negative integer`);
  if (process.env.ARCHITECTURE_STAGE !== undefined) {
    const supplied = Number.parseInt(process.env.ARCHITECTURE_STAGE, 10);
    assert(supplied === currentStage, `ARCHITECTURE_STAGE ${process.env.ARCHITECTURE_STAGE} differs from repository stage ${currentStage}`);
  }
  return currentStage;
}

export function assertOwnedExpiry(entry, context, currentStage) {
  assert(entry && typeof entry === "object" && !Array.isArray(entry), `${context} must be an object`);
  assert(typeof entry.owner === "string" && entry.owner.trim(), `${context}.owner is required`);
  const expiry = entry.expiry_stage ?? entry.expiry_shard ?? entry.delete_shard;
  assert(
    (typeof expiry === "number" && Number.isInteger(expiry)) ||
      (typeof expiry === "string" && expiry.trim()),
    `${context} requires expiry_stage, expiry_shard, or delete_shard`,
  );
  assert(Number.isInteger(currentStage) && currentStage >= 0, `${context} requires an authoritative current stage`);
  if (typeof expiry === "number") assert(currentStage < expiry, `${context} expired at architecture stage ${expiry}`);
}

export function currentRevision() {
  return command("git", ["rev-parse", "HEAD"]).trim();
}

export function runMain(main) {
  Promise.resolve()
    .then(main)
    .catch((error) => {
      console.error(`[architecture] ${error.message}`);
      process.exitCode = 1;
    });
}
