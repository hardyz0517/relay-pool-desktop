import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const REPO_ROOT = resolve(import.meta.dirname, "..");
const CARGO_MANIFEST = resolve(REPO_ROOT, "src-tauri/Cargo.toml");

export function validateReleaseMetadata({
  packageVersion,
  cargoVersion,
  tauriVersion,
  releaseTag,
  requireTag = false,
}) {
  if (tauriVersion !== "../package.json") {
    throw new Error("src-tauri/tauri.conf.json must use ../package.json as its version source");
  }
  if (packageVersion !== cargoVersion) {
    throw new Error(
      `package.json version ${packageVersion} does not match Cargo version ${cargoVersion}`,
    );
  }
  if (requireTag) {
    const expected = `v${packageVersion}`;
    if (!releaseTag) throw new Error("RELAY_POOL_RELEASE_TAG is required for a tagged release");
    if (releaseTag !== expected) {
      throw new Error(`release tag ${releaseTag} does not match source version ${expected}`);
    }
  }
  return `v${packageVersion}`;
}

function cargoPackageVersion(packageName) {
  const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
  const result = spawnSync(
    cargo,
    [
      "metadata",
      "--manifest-path",
      CARGO_MANIFEST,
      "--format-version",
      "1",
      "--no-deps",
      "--locked",
    ],
    { cwd: REPO_ROOT, encoding: "utf8", shell: false },
  );
  if (result.status !== 0) {
    const detail = result.stderr.trim() || result.error?.message || "unknown Cargo error";
    throw new Error(`cargo metadata --locked failed: ${detail}`);
  }
  const metadata = JSON.parse(result.stdout);
  const packages = metadata.packages.filter(
    (candidate) =>
      candidate.name === packageName && resolve(candidate.manifest_path) === CARGO_MANIFEST,
  );
  if (packages.length !== 1) {
    throw new Error("Cargo metadata did not contain exactly one desktop root package");
  }
  return packages[0].version;
}

async function main(argv) {
  let requireTag = false;
  for (const argument of argv) {
    if (argument === "--require-tag") requireTag = true;
    else throw new Error(`unknown argument: ${argument}`);
  }

  const pkg = JSON.parse(await readFile(resolve(REPO_ROOT, "package.json"), "utf8"));
  const tauri = JSON.parse(
    await readFile(resolve(REPO_ROOT, "src-tauri/tauri.conf.json"), "utf8"),
  );
  const verified = validateReleaseMetadata({
    packageVersion: pkg.version,
    cargoVersion: cargoPackageVersion(pkg.name),
    tauriVersion: tauri.version,
    releaseTag: process.env.RELAY_POOL_RELEASE_TAG,
    requireTag,
  });
  console.log(`release version verified: ${verified}`);
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
