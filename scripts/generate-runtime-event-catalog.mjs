import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const check = process.argv.slice(2).includes("--check");
const unknown = process.argv.slice(2).filter((argument) => argument !== "--check");
assert.deepEqual(unknown, [], `unknown runtime catalog arguments: ${unknown.join(", ")}`);

const generatedName = "runtime-event-catalog.v1.json";
const trackedPath = path.join("src-tauri", "generated", generatedName);

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
      "observability::runtime::catalog::tests::emit_runtime_event_catalog",
      "--",
      "--exact",
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        CARGO_TARGET_DIR:
          process.env.CARGO_TARGET_DIR ?? path.join(repoRoot, "output", "cargo", "runtime-event-catalog"),
        RELAY_POOL_RUNTIME_EVENT_CATALOG_OUT: outputDirectory,
      },
      stdio: "inherit",
    },
  );
}

function normalizeLineEndings(bytes) {
  return Buffer.from(bytes.toString("utf8").replaceAll("\r\n", "\n"), "utf8");
}

const temporaryRoot = fs.mkdtempSync(path.join(os.tmpdir(), "relay-pool-runtime-catalog-"));
try {
  const first = path.join(temporaryRoot, "first");
  const second = path.join(temporaryRoot, "second");
  generateInto(first);
  generateInto(second);

  const firstBytes = normalizeLineEndings(fs.readFileSync(path.join(first, generatedName)));
  const secondBytes = normalizeLineEndings(fs.readFileSync(path.join(second, generatedName)));
  assert.deepEqual(
    secondBytes,
    firstBytes,
    `${generatedName} is not deterministic across two clean generations`,
  );

  const outputPath = path.join(repoRoot, trackedPath);
  if (check) {
    assert.ok(fs.existsSync(outputPath), `${trackedPath} is missing; run pnpm generate:runtime-event-catalog`);
    assert.deepEqual(
      normalizeLineEndings(fs.readFileSync(outputPath)),
      firstBytes,
      `${trackedPath} has drifted; run pnpm generate:runtime-event-catalog`,
    );
  } else {
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, firstBytes);
  }
} finally {
  fs.rmSync(temporaryRoot, { recursive: true, force: true });
}

console.log(`Runtime event catalog ${check ? "check" : "generation"} passed (two-run deterministic)`);
