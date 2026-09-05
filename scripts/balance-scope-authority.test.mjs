import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const productionFiles = [];
await collect(path.join(root, "src"), productionFiles);
await collect(path.join(root, "src-tauri", "src"), productionFiles);

const forbiddenAggregateHits = [];
const forbiddenSumHits = [];
for (const file of productionFiles) {
  const relative = path.relative(root, file).replaceAll(path.sep, "/");
  if (
    relative.startsWith("src-tauri/src/persistence/migrations/") ||
    relative.includes(".test.")
  ) continue;
  const source = await readFile(file, "utf8");
  if (
    source.includes("station_key_balance_aggregate") &&
    relative !== "src-tauri/src/services/data_store/alerting_upgrade.rs"
  ) {
    forbiddenAggregateHits.push(relative);
  }
  if (/sum_present_values|sum\s*\([^)]*key_balances/iu.test(source)) forbiddenSumHits.push(relative);
}

assert.deepEqual(
  forbiddenAggregateHits,
  [],
  "legacy station_key_balance_aggregate may only appear in the classification migration",
);
assert.deepEqual(
  forbiddenSumHits,
  [],
  "production code must not derive a station balance by summing key balances",
);

const operationalQuery = await readFile(
  path.join(root, "src-tauri/src/persistence/stores/operational_facts/queries.rs"),
  "utf8",
);
const alertingUpgrade = await readFile(
  path.join(root, "src-tauri/src/services/data_store/alerting_upgrade.rs"),
  "utf8",
);
assert.match(
  alertingUpgrade,
  /latest\.source\s*<>\s*'station_key_balance_aggregate'/u,
  "legacy alerting rebuild must explicitly exclude historical aggregate rows",
);
assert.match(operationalQuery, /balance_kind/u, "operational facts must select typed balance kind");
assert.match(
  operationalQuery,
  /selected\.scope_rank\s*=\s*0[\s\S]*NOT EXISTS[\s\S]*key_current/u,
  "operational facts must prefer the key fact and only fall back to account facts when absent",
);

const assembler = await readFile(
  path.join(root, "src-tauri/src/application/operational_facts/assembler.rs"),
  "utf8",
);
assert.match(
  assembler,
  /balance_fact_rejection_reason[\s\S]*confirmed[\s\S]*authoritative[\s\S]*balance_stale/u,
  "operational planning must fail closed on untrusted or stale balance evidence",
);

const frontend = await readFile(path.join(root, "src/lib/projections/balanceFacts.ts"), "utf8");
assert.match(
  frontend,
  /balanceKind\s*===\s*"account_balance"[\s\S]*evidenceConfidence/u,
  "frontend current balance projection must validate kind and authority",
);
assert.equal(
  frontend.includes("stations.balanceCny") || frontend.includes("station.balanceCny"),
  false,
  "current frontend balance projection must not use compatibility station cache",
);

console.log("balance scope authority gate passed");

async function collect(directory, files) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      if (["target", "node_modules", "dist"].includes(entry.name)) continue;
      await collect(absolute, files);
    } else if (/\.(rs|ts|tsx)$/u.test(entry.name)) {
      files.push(absolute);
    }
  }
}
