import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const validator = readFileSync("scripts/routing-operational-qualification.mjs", "utf8");
const contractsRunner = readFileSync("scripts/run-contract-tests.mjs", "utf8");
const template = readFileSync(
  "docs/audits/routing-operational-qualification-template.md",
  "utf8",
);

assert.ok(
  validator.includes("currentRevision") &&
    validator.includes("soak report sourceRevision must match current HEAD"),
  "routing operational self-check must reject stale soak artifacts from older commits",
);

assert.ok(
  validator.includes("scale baseline source_revision must match current HEAD"),
  "routing operational self-check must reject stale optional scale-baseline artifacts when present",
);

assert.ok(
  validator.includes("--require-long-soak") &&
    !validator.includes("--allow-smoke") &&
    validator.includes("required_default_duration_minutes"),
  "routing operational self-check must keep smoke as the default development check and long soak as optional confidence evidence",
);

assert.ok(
  template.includes("rerun") &&
    template.includes("single-pass deterministic loopback smoke") &&
    template.includes("--require-long-soak"),
  "routing operational self-check template must document stale-artifact reruns and optional long soak",
);

assert.ok(
  contractsRunner.includes("scripts/routing-operational-qualification-boundary.test.mjs"),
  "routing operational self-check boundary test must be registered in contract tests",
);

console.log("routing operational self-check boundary contract passed");
