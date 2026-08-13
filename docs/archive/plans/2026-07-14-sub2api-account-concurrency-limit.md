# Sub2API Account Concurrency Limit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Collect the Sub2API account-level concurrency limit shown on the upstream `/profile` page and display it beside the existing balance cards, only for Sub2API stations.

**Architecture:** Treat the upstream value as a station account/profile fact, not as Relay Pool collector concurrency and not as a key-level setting. Persist it on the station-scope `balance_snapshots` row as `account_concurrency_limit` so the existing station detail balance projection can render it next to balance. The Sub2API balance collector should fetch account profile metadata for Sub2API stations even when key-level `/v1/usage` succeeds, then merge the account concurrency value into the latest station-scope balance snapshot.

**Tech Stack:** Rust/Tauri backend, SQLite, `serde` camelCase serialization, React/TypeScript view models, Node source-contract tests, Cargo tests.

---

## File Structure

- Modify `src-tauri/src/models/pricing.rs`
  - Add `account_concurrency_limit: Option<i64>` to `BalanceSnapshot` and `UpsertBalanceSnapshotInput`.
- Modify `src-tauri/src/services/collectors/facts.rs`
  - Add `account_concurrency_limit: Option<i64>` to `CollectedBalanceFact`.
- Modify `src-tauri/src/services/database.rs`
  - Add SQLite column `balance_snapshots.account_concurrency_limit INTEGER` in table creation and migration.
  - Include the column in insert/update/select row mapping.
- Modify `src-tauri/src/services/collectors/apply.rs`
  - Carry `account_concurrency_limit` from collected balance facts into balance snapshots.
  - Preserve the account-level value when station-key balances are aggregated into a station-scope row.
- Modify `src-tauri/src/services/collectors/adapters/sub2api.rs`
  - Parse profile fields such as `concurrency_limit`, `concurrent_limit`, `concurrency`, `request_concurrency`, `parallel_limit`, and camelCase equivalents.
  - Always try Sub2API account profile collection during `balance` collection, not only when `/v1/usage` fails, so the value is captured for stations that already have working keys.
- Modify `src/lib/types/economics.ts`
  - Add `accountConcurrencyLimit: number | null` to `BalanceSnapshot`.
- Modify `src/features/stations/stationDetailViewModels.ts`
  - Add one extra balance card only when `station.stationType === "sub2api"` and the latest station balance snapshot contains a finite account concurrency limit.
- Modify `scripts/station-auto-collector.test.mjs`
  - Add a source-contract assertion that Sub2API account profile collection is not only a no-key fallback.
- Create `scripts/station-detail-account-concurrency-card.test.mjs`
  - Assert the card appears beside existing balance cards only for Sub2API stations.

---

### Task 1: Add The Persisted Account Concurrency Field

**Files:**
- Modify: `src-tauri/src/models/pricing.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src/lib/types/economics.ts`

- [ ] **Step 1: Write the failing Rust serialization test**

Add this test inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/models/pricing.rs`:

```rust
#[test]
fn balance_snapshot_serializes_account_concurrency_limit() {
    let snapshot = BalanceSnapshot {
        id: "balance-1".to_string(),
        station_id: "station-1".to_string(),
        station_key_id: None,
        scope: "station".to_string(),
        value: Some(66.78),
        currency: "CNY".to_string(),
        credit_unit: None,
        used_value: None,
        total_value: None,
        today_request_count: None,
        total_request_count: None,
        today_consumption: None,
        total_consumption: None,
        today_base_consumption: None,
        total_base_consumption: None,
        today_token_count: None,
        total_token_count: None,
        today_input_token_count: None,
        today_output_token_count: None,
        total_input_token_count: None,
        total_output_token_count: None,
        account_concurrency_limit: Some(5),
        low_balance_threshold: None,
        status: "normal".to_string(),
        source: "sub2api_profile".to_string(),
        confidence: 0.9,
        collected_at: Some("2026-07-14T08:00:00.000Z".to_string()),
        created_at: "2026-07-14T08:00:00.000Z".to_string(),
        updated_at: "2026-07-14T08:00:00.000Z".to_string(),
    };

    let json = serde_json::to_value(snapshot).expect("json");
    assert_eq!(json["accountConcurrencyLimit"], 5);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml balance_snapshot_serializes_account_concurrency_limit -- --nocapture
```

Expected: FAIL because `BalanceSnapshot` does not yet have `account_concurrency_limit`.

- [ ] **Step 3: Add the Rust and TypeScript types**

In `src-tauri/src/models/pricing.rs`, add this field to both `BalanceSnapshot` and `UpsertBalanceSnapshotInput`, immediately before `low_balance_threshold`:

```rust
pub account_concurrency_limit: Option<i64>,
```

In `src/lib/types/economics.ts`, add the matching frontend field immediately before `lowBalanceThreshold`:

```ts
accountConcurrencyLimit: number | null;
```

- [ ] **Step 4: Update SQLite schema creation and migration**

In both `CREATE TABLE IF NOT EXISTS balance_snapshots` blocks in `src-tauri/src/services/database.rs`, add this column before `low_balance_threshold`:

```sql
account_concurrency_limit INTEGER,
```

In `ensure_schema` near the other `balance_snapshots` `add_column_if_missing` calls, add:

```rust
add_column_if_missing(
    connection,
    "balance_snapshots",
    "account_concurrency_limit",
    "INTEGER",
)?;
```

- [ ] **Step 5: Update balance snapshot insert/update and row mapping**

In `upsert_balance_snapshot_in_connection`, include `account_concurrency_limit` in the insert column list after `total_output_token_count`, add one placeholder before `low_balance_threshold`, update the conflict assignment, and pass `input.account_concurrency_limit` in `params!`.

The relevant column span should become:

```rust
today_input_token_count, today_output_token_count, total_input_token_count,
total_output_token_count, account_concurrency_limit, low_balance_threshold,
status, source, confidence, collected_at, created_at, updated_at
```

The conflict assignment should include:

```rust
account_concurrency_limit = excluded.account_concurrency_limit,
```

In `row_to_balance_snapshot`, map the new column:

```rust
account_concurrency_limit: row.get("account_concurrency_limit")?,
```

- [ ] **Step 6: Run focused database/model checks**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml balance_snapshot_serializes_account_concurrency_limit -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml station_detail_balance_snapshots_are_filtered_in_database -- --nocapture
```

Expected: both PASS.

---

### Task 2: Carry The Field Through Collector Facts

**Files:**
- Modify: `src-tauri/src/services/collectors/facts.rs`
- Modify: `src-tauri/src/services/collectors/apply.rs`
- Modify: any existing Rust test builders in `src-tauri/src/services/collectors/apply.rs` that construct `CollectedBalanceFact`

- [ ] **Step 1: Write the failing apply-layer test**

Add this test inside `#[cfg(test)] mod tests` in `src-tauri/src/services/collectors/apply.rs`:

```rust
#[test]
fn applies_account_concurrency_limit_to_station_balance_snapshot() {
    let database = crate::services::database::tests::create_test_database();
    let station = database
        .create_station(crate::models::stations::StationInput {
            name: "Sub2 Account".to_string(),
            station_type: "sub2api".to_string(),
            website_url: "https://example.test".to_string(),
            api_base_url: "https://example.test/v1".to_string(),
            api_key: "".to_string(),
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            collection_interval_minutes: 5,
            note: None,
        })
        .expect("station");

    let mut facts = crate::services::collectors::facts::CollectorFacts::default();
    facts.balances.push(crate::services::collectors::facts::CollectedBalanceFact {
        station_id: station.id.clone(),
        station_key_id: None,
        scope: "station".to_string(),
        value: Some(66.78),
        used_value: None,
        total_value: None,
        today_request_count: None,
        total_request_count: None,
        today_consumption: None,
        total_consumption: None,
        today_base_consumption: None,
        total_base_consumption: None,
        today_token_count: None,
        total_token_count: None,
        today_input_token_count: None,
        today_output_token_count: None,
        total_input_token_count: None,
        total_output_token_count: None,
        account_concurrency_limit: Some(5),
        currency: "CNY".to_string(),
        credit_unit: None,
        status: "normal".to_string(),
        source: "sub2api_profile".to_string(),
        confidence: 0.9,
        collected_at: None,
    });

    super::apply_collector_facts(&database, facts).expect("apply facts");
    let balances = database
        .list_balance_snapshots_for_station(station.id)
        .expect("balances");
    assert_eq!(balances[0].account_concurrency_limit, Some(5));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml applies_account_concurrency_limit_to_station_balance_snapshot -- --nocapture
```

Expected: FAIL until `CollectedBalanceFact` and apply code carry the new field.

- [ ] **Step 3: Add the field to collector facts**

In `src-tauri/src/services/collectors/facts.rs`, add this field to `CollectedBalanceFact` immediately before `currency`:

```rust
pub account_concurrency_limit: Option<i64>,
```

- [ ] **Step 4: Pass the field into balance snapshot upserts**

In both balance upsert paths in `src-tauri/src/services/collectors/apply.rs`, add:

```rust
account_concurrency_limit: balance.account_concurrency_limit,
```

Place it before `low_balance_threshold: None`.

- [ ] **Step 5: Preserve the value during station-key aggregation**

In `append_station_balance_aggregates`, include `account_concurrency_limit` in the tuple collected from `key_balances`. Since account concurrency is account-level and key balances should not set it, use the first present value if any exists:

```rust
let account_concurrency_limit = key_balances
    .iter()
    .find_map(|balance| balance.account_concurrency_limit);
```

Add it to the tuple and set it on the generated station aggregate:

```rust
account_concurrency_limit,
```

Every existing `CollectedBalanceFact` construction must now set `account_concurrency_limit: None` unless it is intentionally carrying the profile value.

- [ ] **Step 6: Run focused apply tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml applies_account_concurrency_limit_to_station_balance_snapshot -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml station_key_balance_aggregation -- --nocapture
```

Expected: the new test passes; existing aggregation tests still pass after adding `account_concurrency_limit: None` in builders.

---

### Task 3: Collect And Parse Sub2API Profile Concurrency

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/sub2api.rs`
- Modify: `scripts/station-auto-collector.test.mjs`

- [ ] **Step 1: Add parser tests for the Sub2API profile field**

Add these tests to the existing `#[cfg(test)] mod tests` in `src-tauri/src/services/collectors/adapters/sub2api.rs`:

```rust
#[test]
fn parse_account_balance_reads_profile_concurrency_limit() {
    let fact = parse_account_balance(
        "station-1",
        &json!({
            "data": {
                "balance": 66.78,
                "concurrency_limit": 5
            }
        }),
        1.0,
    )
    .expect("profile fact");

    assert_eq!(fact.value, Some(66.78));
    assert_eq!(fact.account_concurrency_limit, Some(5));
    assert_eq!(fact.scope, "station");
}

#[test]
fn parse_account_balance_accepts_common_concurrency_aliases() {
    for (field, expected) in [
        ("concurrent_limit", 4),
        ("concurrency", 5),
        ("request_concurrency", 6),
        ("parallel_limit", 7),
        ("max_concurrency", 8),
        ("concurrencyLimit", 9),
    ] {
        let fact = parse_account_balance(
            "station-1",
            &json!({
                "data": {
                    "balance": 1.0,
                    field: expected
                }
            }),
            1.0,
        )
        .expect("profile fact");
        assert_eq!(fact.account_concurrency_limit, Some(expected), "field {field}");
    }
}
```

- [ ] **Step 2: Run parser tests to verify they fail**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml parse_account_balance_reads_profile_concurrency_limit parse_account_balance_accepts_common_concurrency_aliases -- --nocapture
```

Expected: FAIL because profile parsing does not yet set `account_concurrency_limit`.

- [ ] **Step 3: Add the parser helper**

In `src-tauri/src/services/collectors/adapters/sub2api.rs`, add this helper near `parse_i64_field`:

```rust
fn parse_account_concurrency_limit(payload: &Value) -> Option<i64> {
    parse_i64_field(
        payload,
        &[
            "concurrency_limit",
            "concurrent_limit",
            "concurrency",
            "request_concurrency",
            "parallel_limit",
            "max_concurrency",
            "concurrencyLimit",
            "concurrentLimit",
            "requestConcurrency",
            "parallelLimit",
            "maxConcurrency",
        ],
    )
    .filter(|value| *value > 0)
}
```

In `parse_account_balance`, set:

```rust
account_concurrency_limit: parse_account_concurrency_limit(payload),
```

All other `CollectedBalanceFact` values in this file should set `account_concurrency_limit: None` except facts derived from profile payloads.

- [ ] **Step 4: Make Sub2API balance collection fetch account profile metadata even when key usage succeeds**

Change `collect_account_balance_fallback` into a profile collection helper that can return a station-scope profile fact whenever `/api/v1/user/profile` or `/api/v1/auth/me` succeeds. Keep the existing behavior that returns a full station balance fact when key-level usage produced no balance.

Use this shape:

```rust
fn collect_account_profile_fact(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station: &crate::models::stations::Station,
    proxy: &ProxyConfig,
    endpoint_results: &mut Vec<Value>,
    budget: &CollectionAttemptBudget,
    policy: &RequestPolicy,
) -> Result<Option<CollectedBalanceFact>, String> {
    // Reuse the existing token/session/auth-refresh logic from collect_account_balance_fallback.
    // On a successful profile payload, return parse_account_balance(...).
}
```

Then in `collect_balance`, after key `/v1/usage` attempts and before dashboard stats, always call this helper for Sub2API:

```rust
let account_profile_balance = collect_account_profile_fact(
    database,
    data_key,
    &station,
    &proxy,
    &mut endpoint_results,
    &budget,
    &policy,
)?;

if facts.balances.is_empty() {
    if let Some(balance) = account_profile_balance.clone() {
        facts.balances.push(balance);
    }
} else if let Some(profile) = account_profile_balance {
    merge_account_profile_into_station_balance(&mut facts.balances, profile);
}
```

Add this merge helper near `merge_dashboard_usage_stats`:

```rust
fn merge_account_profile_into_station_balance(
    balances: &mut Vec<CollectedBalanceFact>,
    profile: CollectedBalanceFact,
) {
    let Some(limit) = profile.account_concurrency_limit else {
        return;
    };

    if let Some(station_balance) = balances
        .iter_mut()
        .find(|balance| balance.station_id == profile.station_id && balance.scope == "station")
    {
        station_balance.account_concurrency_limit = Some(limit);
        return;
    }

    balances.push(CollectedBalanceFact {
        station_id: profile.station_id,
        station_key_id: None,
        scope: "station".to_string(),
        value: profile.value,
        used_value: profile.used_value,
        total_value: profile.total_value,
        today_request_count: profile.today_request_count,
        total_request_count: profile.total_request_count,
        today_consumption: profile.today_consumption,
        total_consumption: profile.total_consumption,
        today_base_consumption: profile.today_base_consumption,
        total_base_consumption: profile.total_base_consumption,
        today_token_count: profile.today_token_count,
        total_token_count: profile.total_token_count,
        today_input_token_count: profile.today_input_token_count,
        today_output_token_count: profile.today_output_token_count,
        total_input_token_count: profile.total_input_token_count,
        total_output_token_count: profile.total_output_token_count,
        account_concurrency_limit: Some(limit),
        currency: profile.currency,
        credit_unit: profile.credit_unit,
        status: profile.status,
        source: profile.source,
        confidence: profile.confidence,
        collected_at: profile.collected_at,
    });
}
```

- [ ] **Step 5: Add a source-contract test that profile collection is not fallback-only**

In `scripts/station-auto-collector.test.mjs`, add:

```js
assert.ok(
  sub2apiSource.includes("collect_account_profile_fact") &&
    /let\s+account_profile_balance\s*=\s*collect_account_profile_fact[\s\S]*?if\s*\(facts\.balances\.is_empty\(\)\)/.test(sub2apiSource),
  "Sub2API balance collection should collect account profile metadata before falling back on missing key balances",
);
```

- [ ] **Step 6: Run focused collector tests**

Run:

```powershell
node scripts\station-auto-collector.test.mjs
cargo test --manifest-path .\src-tauri\Cargo.toml parse_account_balance_reads_profile_concurrency_limit parse_account_balance_accepts_common_concurrency_aliases -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api -- --nocapture
```

Expected: all PASS. Any failing test builders must be updated with `account_concurrency_limit: None`.

---

### Task 4: Display The Sub2API-Only Card Beside Balance

**Files:**
- Modify: `src/features/stations/stationDetailViewModels.ts`
- Create: `scripts/station-detail-account-concurrency-card.test.mjs`

- [ ] **Step 1: Write the failing frontend view-model test**

Create `scripts/station-detail-account-concurrency-card.test.mjs`:

```js
import assert from "node:assert/strict";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import vm from "node:vm";
import ts from "typescript";

const root = process.cwd();
const require = createRequire(import.meta.url);
const sourcePath = path.join(root, "src", "features", "stations", "stationDetailViewModels.ts");
const source = fs.readFileSync(sourcePath, "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.CommonJS,
    target: ts.ScriptTarget.ES2022,
  },
});

const module = { exports: {} };
vm.runInNewContext(compiled.outputText, {
  exports: module.exports,
  module,
  require: (specifier) => {
    if (specifier === "@/lib/projections/balanceFacts") {
      return {
        currentStationBalanceFor({ station, balances }) {
          const sourceSnapshot =
            balances.find((balance) => balance.stationId === station.id && balance.scope === "station") ?? null;
          return {
            sourceSnapshot,
            collectedAt: sourceSnapshot?.collectedAt ?? null,
            currency: sourceSnapshot?.currency ?? "CNY",
            value: sourceSnapshot?.value ?? null,
            lowBalanceThreshold: sourceSnapshot?.lowBalanceThreshold ?? null,
            status: sourceSnapshot?.status ?? "unknown",
            source: sourceSnapshot ? "balance_snapshot" : "missing",
            sourceLabel: sourceSnapshot ? sourceSnapshot.source : "missing",
            updatedAt: sourceSnapshot?.updatedAt ?? null,
          };
        },
      };
    }
    if (specifier === "@/lib/projections/groupFacts") {
      return {
        buildCurrentStationGroupFacts: () => [],
        isDisplayableStationGroupCurrentFact: () => false,
      };
    }
    if (specifier === "@/lib/time") {
      return { toTimestampMillis: (value) => Number(value) || Date.parse(value) };
    }
    if (specifier === "@/lib/formatters") {
      return { formatTrimmedDecimal: (value, digits = 2) => Number(value).toFixed(digits).replace(/\.?0+$/, "") };
    }
    return require(specifier);
  },
}, { filename: sourcePath });

const { buildBalanceCards } = module.exports;

const baseBalance = {
  id: "balance-a",
  stationId: "station-a",
  scope: "station",
  value: 66.78,
  currency: "CNY",
  lowBalanceThreshold: null,
  status: "normal",
  source: "sub2api_profile",
  updatedAt: "2026-07-14T08:00:00.000Z",
  collectedAt: "2026-07-14T08:00:00.000Z",
  accountConcurrencyLimit: 5,
};

const sub2apiCards = buildBalanceCards({ id: "station-a", stationType: "sub2api" }, [baseBalance]);
assert.equal(sub2apiCards.find((card) => card.label === "并发限制")?.value, "5 路");
assert.match(
  sub2apiCards.find((card) => card.label === "并发限制")?.helper ?? "",
  /账号资料页|Sub2API/,
);

const newapiCards = buildBalanceCards({ id: "station-a", stationType: "newapi" }, [baseBalance]);
assert.equal(newapiCards.find((card) => card.label === "并发限制"), undefined);

const missingCards = buildBalanceCards({ id: "station-a", stationType: "sub2api" }, [
  { ...baseBalance, accountConcurrencyLimit: null },
]);
assert.equal(missingCards.find((card) => card.label === "并发限制"), undefined);
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
node scripts\station-detail-account-concurrency-card.test.mjs
```

Expected: FAIL because `buildBalanceCards` does not render the card yet.

- [ ] **Step 3: Implement the Sub2API-only balance card**

In `src/features/stations/stationDetailViewModels.ts`, update `buildBalanceCards` to append the card after the existing three balance cards:

```ts
export function buildBalanceCards(station: Station, balances: BalanceSnapshot[]): StationDetailBalanceCard[] {
  const currentBalance = currentStationBalanceFor({ station, balances });
  const currency = currentBalance.currency;
  const currentValue = currentBalance.value;
  const threshold = currentBalance.lowBalanceThreshold;
  const balanceTone = balanceToneFor(currentValue, threshold, currentBalance.status);
  const cards: StationDetailBalanceCard[] = [
    // keep the existing three cards unchanged
  ];

  const accountConcurrencyLimit = currentBalance.sourceSnapshot?.accountConcurrencyLimit;
  if (
    station.stationType === "sub2api" &&
    typeof accountConcurrencyLimit === "number" &&
    Number.isFinite(accountConcurrencyLimit) &&
    accountConcurrencyLimit > 0
  ) {
    cards.push({
      label: "并发限制",
      value: `${accountConcurrencyLimit} 路`,
      helper: currentBalance.collectedAt
        ? `来自 Sub2API 账号资料页：${formatDetailDate(currentBalance.collectedAt)}`
        : "来自 Sub2API 账号资料页",
      tone: "neutral",
    });
  }

  return cards;
}
```

Keep the existing card labels and helper text untouched except for moving the array into a `cards` variable.

- [ ] **Step 4: Keep the grid stable when a fourth card exists**

In `src/features/stations/components/StationDetailContent.tsx`, change the balance card grid from `md:grid-cols-3` to a responsive four-column layout:

```tsx
<div className="grid gap-3 p-4 md:grid-cols-2 xl:grid-cols-4">
```

This keeps the existing three-card case compact and lets the fourth Sub2API card sit beside the balance cards on wide windows.

- [ ] **Step 5: Run frontend focused tests**

Run:

```powershell
node scripts\station-detail-account-concurrency-card.test.mjs
node scripts\station-detail-token-compact-format.test.mjs
.\node_modules\.bin\tsc.cmd --noEmit
```

Expected: all PASS.

---

### Task 5: Final Verification

**Files:**
- Verify only; no new files.

- [ ] **Step 1: Run backend checks**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml parse_account_balance_reads_profile_concurrency_limit parse_account_balance_accepts_common_concurrency_aliases -- --nocapture
cargo test --manifest-path .\src-tauri\Cargo.toml applies_account_concurrency_limit_to_station_balance_snapshot -- --nocapture
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: all PASS.

- [ ] **Step 2: Run frontend/source-contract checks**

Run:

```powershell
node scripts\station-auto-collector.test.mjs
node scripts\station-detail-account-concurrency-card.test.mjs
node scripts\station-detail-token-compact-format.test.mjs
.\node_modules\.bin\tsc.cmd --noEmit
.\node_modules\.bin\vite.cmd build
```

Expected: all PASS.

- [ ] **Step 3: Manual UI verification in the Tauri app**

Run:

```powershell
pnpm tauri:dev
```

Manual checks:

- Open a Sub2API station detail page after running balance/full collection.
- Confirm the balance section shows `并发限制` beside the balance cards when the upstream profile returns a positive value.
- Confirm a NewAPI or custom station detail page does not show `并发限制`.
- Confirm the value is not shown under `采集任务`, not shown in key rows, and not editable in the provider form.

- [ ] **Step 4: Stage exact paths only if committing**

If the user asks to commit, stage only the touched paths:

```powershell
git add -- src-tauri/src/models/pricing.rs src-tauri/src/services/database.rs src-tauri/src/services/collectors/facts.rs src-tauri/src/services/collectors/apply.rs src-tauri/src/services/collectors/adapters/sub2api.rs src/lib/types/economics.ts src/features/stations/stationDetailViewModels.ts src/features/stations/components/StationDetailContent.tsx scripts/station-auto-collector.test.mjs scripts/station-detail-account-concurrency-card.test.mjs
git diff --cached --name-only
```

Expected staged files:

```text
scripts/station-auto-collector.test.mjs
scripts/station-detail-account-concurrency-card.test.mjs
src-tauri/src/models/pricing.rs
src-tauri/src/services/collectors/adapters/sub2api.rs
src-tauri/src/services/collectors/apply.rs
src-tauri/src/services/collectors/facts.rs
src-tauri/src/services/database.rs
src/features/stations/components/StationDetailContent.tsx
src/features/stations/stationDetailViewModels.ts
src/lib/types/economics.ts
```

---

## Self-Review

- Scope: This plan adds one Sub2API-only account profile field, stores it with station balance facts, and renders it beside the existing balance cards. It does not add user-editable collector concurrency settings.
- Sub2API-only guard: Backend collection occurs inside the Sub2API adapter; frontend display requires `station.stationType === "sub2api"`.
- UI placement: The field is in the existing balance section, not in `采集任务`, key rows, group rates, or the edit form.
- Data flow: `/api/v1/user/profile` or `/api/v1/auth/me` -> `CollectedBalanceFact.account_concurrency_limit` -> `balance_snapshots.account_concurrency_limit` -> `BalanceSnapshot.accountConcurrencyLimit` -> `buildBalanceCards`.
- Verification: The plan includes Rust parser/apply checks, a source-contract check that profile collection is not fallback-only, frontend view-model checks, TypeScript, Vite, Cargo, and manual Tauri UI verification.
