import fs from "node:fs";
import path from "node:path";
import { assert, readJson, repoRoot, runMain } from "./lib.mjs";

runMain(() => {
  const outputIndex = process.argv.indexOf("--output");
  assert(outputIndex >= 0 && process.argv[outputIndex + 1], "--output <path> is required");
  const output = path.resolve(repoRoot, process.argv[outputIndex + 1]);
  assert(output.startsWith(path.join(repoRoot, "output") + path.sep), "generated cargo-deny config must be under output/");
  const exceptions = readJson("docs/audits/dependency-advisory-exceptions.json").exceptions
    .filter((entry) => entry.ecosystem === "cargo");
  const ids = exceptions.map((entry) => entry.advisory_id);
  assert(ids.every((id) => /^RUSTSEC-\d{4}-\d{4}$/.test(id)), "cargo advisory exceptions require exact RUSTSEC ids");
  const base = fs.readFileSync(path.join(repoRoot, "deny.toml"), "utf8");
  const advisoryStart = base.indexOf("[advisories]");
  assert(advisoryStart >= 0, "deny.toml must contain [advisories]");
  const nextSection = base.indexOf("\n[", advisoryStart + "[advisories]".length);
  const advisorySection = base.slice(advisoryStart, nextSection < 0 ? base.length : nextSection);
  assert(!/^\s*ignore\s*=/m.test(advisorySection), "deny.toml [advisories] must not contain a static/global ignore");
  const rendered = base.replace("[advisories]", `[advisories]\nignore = [${ids.map((id) => `"${id}"`).join(", ")}]`);
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, rendered, "utf8");
  console.log(output);
});
