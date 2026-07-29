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
const UNRESOLVED_PATH_TOKEN = /(^~(?:$|[\\/]))|%[^%]+%|\$[A-Za-z_][A-Za-z0-9_]*/;

function inventoryArtifactEntries(inventory) {
  const raw = inventory.inventories?.artifacts ?? inventory.inventories?.artifact_directories ?? [];
  return Array.isArray(raw) ? raw : Object.values(raw);
}

function validateArtifactPath(rawPath, label) {
  assert(typeof rawPath === "string" && rawPath.trim(), `${label} path must be a non-empty string`);
  assert(!UNRESOLVED_PATH_TOKEN.test(rawPath), `${label} path must not contain unresolved home or environment tokens: ${rawPath}`);
  assert(!path.isAbsolute(rawPath), `${label} path must be repository-relative: ${rawPath}`);

  const normalized = normalizePath(rawPath);
  assert(normalized !== "." && normalized !== "", `${label} path must not resolve to the workspace root`);
  assert(normalized !== ".." && !normalized.startsWith("../") && !normalized.includes("/../"), `${label} path must not traverse outside the workspace: ${rawPath}`);

  const absolute = path.resolve(repoRoot, normalized);
  const relative = normalizePath(path.relative(repoRoot, absolute));
  assert(relative !== "." && relative !== "", `${label} path must not resolve to the workspace root`);
  assert(relative !== ".." && !relative.startsWith("../") && !path.isAbsolute(relative), `${label} path must resolve inside the workspace: ${rawPath}`);
  assert(relative === normalized, `${label} path must be normalized: ${rawPath}`);
  return normalized;
}

function assertRejectsArtifactPath(rawPath, reason) {
  let rejected = false;
  try {
    validateArtifactPath(rawPath, "fixture");
  } catch {
    rejected = true;
  }
  assert(rejected, reason);
}

function inventoryArtifactPaths(inventory) {
  return new Set(
    inventoryArtifactEntries(inventory).map((entry, index) =>
      validateArtifactPath(typeof entry === "string" ? entry : entry.path, `inventories.artifacts[${index}]`),
    ),
  );
}

runMain(() => {
  if (process.argv.includes("--fixtures")) {
    assert(FORBIDDEN_SOURCE_DIR.test("src/features/example/output/result.json"), "source output bypass fixture must be detected");
    assert(FORBIDDEN_SOURCE_DIR.test("src-tauri/src/services/target/result.bin"), "Rust source target bypass fixture must be detected");
    assert(!FORBIDDEN_SOURCE_DIR.test("output/architecture-scale/result.json"), "approved root output must remain allowed");
    assert(
      validateArtifactPath("output/architecture-scale/result.json", "fixture") === "output/architecture-scale/result.json",
      "approved output artifact path should normalize unchanged",
    );
    assertRejectsArtifactPath("D:/tmp/result.json", "absolute Windows paths must be rejected");
    assertRejectsArtifactPath("../outside/result.json", "parent traversal must be rejected");
    assertRejectsArtifactPath("~/.cache/result.json", "home-relative paths must be rejected");
    assertRejectsArtifactPath("%TEMP%/result.json", "unresolved Windows environment paths must be rejected");
    assertRejectsArtifactPath("$TMPDIR/result.json", "unresolved shell environment paths must be rejected");
    assertRejectsArtifactPath(".", "workspace root must be rejected");
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
