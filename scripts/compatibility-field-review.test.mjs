import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const ledger = await readFile("docs/superpowers/audits/relay-pool-field-ownership-ledger.md", "utf8");

assert.ok(
  ledger.includes("## Stage 8 兼容字段复查结论"),
  "field ownership ledger should include the Stage 8 compatibility review conclusion",
);
assert.ok(
  ledger.includes("本轮无 removable candidate 字段"),
  "Stage 8 review should explicitly avoid approving field removal",
);
assert.ok(ledger.includes("`station_keys.group_name`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`station_keys.group_id_hash`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`station_keys.rate_multiplier`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`stations.balance_raw`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`stations.balance_cny`") && ledger.includes("compatibility cache"));
assert.ok(ledger.includes("`stations.last_pricing_fetched_at`") && ledger.includes("compatibility cache"));
assert.ok(!ledger.includes("| `station_keys.group_name` | removable candidate |"));
assert.ok(!ledger.includes("| `station_keys.group_id_hash` | removable candidate |"));
assert.ok(!ledger.includes("| `station_keys.rate_multiplier` | removable candidate |"));
assert.ok(!ledger.includes("| `stations.balance_raw` | removable candidate |"));
assert.ok(!ledger.includes("| `stations.balance_cny` | removable candidate |"));
assert.ok(!ledger.includes("| `stations.last_pricing_fetched_at` | removable candidate |"));
