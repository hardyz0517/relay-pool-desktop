import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const check = process.argv.slice(2).includes("--check");
const unknown = process.argv.slice(2).filter((argument) => argument !== "--check");
assert.deepEqual(unknown, [], `unknown generate-bindings arguments: ${unknown.join(", ")}`);

const artifacts = [
  ["generated.ts", "src/lib/bridge/generated.ts"],
  ["command-registry.json", "src-tauri/generated/command-registry.json"],
  ["pilot-serialization.json", "src-tauri/src/ipc/dto/fixtures/pilot-serialization.json"],
];

function generateInto(outputDirectory) {
  fs.mkdirSync(outputDirectory, { recursive: true });
  execFileSync(
    "cargo",
    [
      "test",
      "--locked",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--lib",
      "ipc::registry::tests::emit_repository_bindings",
      "--",
      "--exact",
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        CARGO_TARGET_DIR:
          process.env.CARGO_TARGET_DIR ?? path.join(repoRoot, "output", "cargo", "binding-generator"),
        RELAY_POOL_BINDINGS_OUT: outputDirectory,
      },
      stdio: "inherit",
    },
  );
}

const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "relay-pool-bindings-"));
try {
  const first = path.join(temporaryRoot, "first");
  const second = path.join(temporaryRoot, "second");
  generateInto(first);
  generateInto(second);

  for (const [generatedName, trackedName] of artifacts) {
    const firstBytes = fs.readFileSync(path.join(first, generatedName));
    const secondBytes = fs.readFileSync(path.join(second, generatedName));
    assert.deepEqual(secondBytes, firstBytes, `${generatedName} is not deterministic across two clean generations`);

    const trackedPath = path.join(repoRoot, trackedName);
    if (check) {
      assert.ok(fs.existsSync(trackedPath), `${trackedName} is missing; run pnpm generate:bindings`);
      assert.deepEqual(
        fs.readFileSync(trackedPath),
        firstBytes,
        `${trackedName} has drifted; run pnpm generate:bindings`,
      );
      continue;
    }
    fs.mkdirSync(path.dirname(trackedPath), { recursive: true });
    fs.writeFileSync(trackedPath, firstBytes);
  }
} finally {
  fs.rmSync(temporaryRoot, { recursive: true, force: true });
}

console.log(`IPC bindings ${check ? "check" : "generation"} passed (${artifacts.length} artifacts, two-run deterministic)`);
