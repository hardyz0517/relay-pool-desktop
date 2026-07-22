import fs from "node:fs";
import path from "node:path";
import { assert, repoRoot, runMain } from "../lib.mjs";
import { canonicalJson, generateFixtureManifest, sha256 } from "./dataset.mjs";

runMain(() => {
  const outputIndex = process.argv.indexOf("--output");
  assert(outputIndex >= 0 && process.argv[outputIndex + 1], "--output <path> is required");
  const output = path.resolve(repoRoot, process.argv[outputIndex + 1]);
  assert(output.startsWith(path.join(repoRoot, "output") + path.sep), "scale fixtures must be written under output/");
  const manifest = generateFixtureManifest();
  const json = canonicalJson(manifest);
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, json, "utf8");
  console.log(JSON.stringify({ path: output, sha256: sha256(json) }));
});
