import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\//, "");
const sourceRoots = [join(root, "src-tauri", "src"), join(root, "src")];
const ignoredSegments = [
  `${join("src-tauri", "src", "persistence", "migrations")}`,
  `${join("src-tauri", "src", "services", "portable_migration", "catalog.rs")}`,
];
const forbidden = [
  "health_writeback_mode",
  "health_writeback_decision",
  "health_writeback_reason",
  "station_keys.status",
  "stations.status",
  "scheduler_advanced_settings_json",
  "default_routing_strategy",
  "default_routing_group_filter",
];

async function walk(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await walk(path)));
    else if (/\.(rs|ts|tsx|js|mjs)$/.test(entry.name)) files.push(path);
  }
  return files;
}

const hits = [];
for (const directory of sourceRoots) {
  for (const file of await walk(directory)) {
    const normalized = relative(root, file).replaceAll("\\", "/");
    if (ignoredSegments.some((segment) => normalized.startsWith(segment.replaceAll("\\", "/")))) continue;
    const text = await readFile(file, "utf8");
    for (const token of forbidden) {
      if (text.includes(token)) hits.push(`${normalized}: ${token}`);
    }
  }
}

assert.equal(
  hits.length,
  0,
  `cutover schema fields still have active source references:\n${hits.join("\n")}`,
);
console.log("routing cutover schema zero-reference check passed");
