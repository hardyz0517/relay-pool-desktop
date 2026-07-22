import fs from "node:fs";
import path from "node:path";
import {
  assert,
  command,
  listFiles,
  normalizePath,
  readRequiredManifest,
  repoRoot,
  runMain,
} from "./lib.mjs";

const FORBIDDEN_SOURCE_DIR = /(?:^|\/)(?:src|src-tauri\/src)(?:\/.*)?\/(?:target|output|dist)(?:\/|$)/;

function inventoryArtifactPaths(inventory) {
  const raw = inventory.inventories?.artifacts ?? inventory.inventories?.artifact_directories ?? [];
  return new Set((Array.isArray(raw) ? raw : Object.values(raw)).map((entry) => normalizePath(typeof entry === "string" ? entry : entry.path)));
}

runMain(() => {
  if (process.argv.includes("--fixtures")) {
    assert(FORBIDDEN_SOURCE_DIR.test("src/features/example/output/result.json"), "source output bypass fixture must be detected");
    assert(FORBIDDEN_SOURCE_DIR.test("src-tauri/src/services/target/result.bin"), "Rust source target bypass fixture must be detected");
    assert(!FORBIDDEN_SOURCE_DIR.test("output/architecture-scale/result.json"), "approved root output must remain allowed");
    console.log("Artifact policy fixtures passed");
    return;
  }
  const inventory = readRequiredManifest("docs/superpowers/audits/architecture-scale-upgrade-inventory.json", ["inventories"]);
  const registered = inventoryArtifactPaths(inventory);
  const tracked = command("git", ["ls-files", "-z"]).split("\0").filter(Boolean).map(normalizePath);
  const trackedArtifacts = tracked.filter((file) => /(?:^|\/)(?:output|target|dist)(?:\/|$)/.test(file));
  const unregisteredTracked = trackedArtifacts.filter((file) => !registered.has(file));
  assert(unregisteredTracked.length === 0, `unregistered generated artifacts are tracked: ${unregisteredTracked.join(", ")}`);
  for (const registeredPath of registered) {
    assert(tracked.includes(registeredPath), `stale registered tracked artifact: ${registeredPath}`);
  }

  const sourceArtifacts = [
    ...listFiles(path.join(repoRoot, "src"), () => true),
    ...listFiles(path.join(repoRoot, "src-tauri/src"), () => true),
  ]
    .map((file) => normalizePath(path.relative(repoRoot, file)))
    .filter((file) => FORBIDDEN_SOURCE_DIR.test(file));
  const unregistered = sourceArtifacts.filter((file) => ![...registered].some((root) => file === root || file.startsWith(`${root}/`)));
  assert(unregistered.length === 0, `unregistered source-tree artifacts: ${unregistered.slice(0, 20).join(", ")}`);

  const ignore = fs.readFileSync(path.join(repoRoot, ".gitignore"), "utf8");
  for (const pattern of ["/output/", "target/", "dist/", "node_modules/"]) {
    assert(ignore.split(/\r?\n/).includes(pattern), `.gitignore must contain exact artifact rule '${pattern}'`);
  }
  const vite = fs.readFileSync(path.join(repoRoot, "vite.config.ts"), "utf8");
  assert(vite.includes('"**/output/**"'), "Vite watch ignore must exclude **/output/**");
  console.log(`Artifact policy gate passed (${registered.size} registered legacy roots)`);
});
