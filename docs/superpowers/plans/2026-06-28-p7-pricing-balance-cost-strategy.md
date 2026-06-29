# P7 Pricing, Balance, and Cost Strategy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade Relay Pool Desktop routing from model/protocol/health-aware selection to cost-aware routing that can compare normalized prices, avoid low-balance stations, estimate request costs, and explain those decisions.

**Architecture:** P7 keeps `Station Key` as the routing object, while treating balance as mostly `Station` account state and pricing as normalized station/group/model rules. Raw collector snapshots remain source material; router/runtime reads normalized pricing and balance tables through focused database APIs, then records cost metadata in request logs without storing prompts, responses, or full API keys.

**Tech Stack:** Tauri 2, Rust, rusqlite, React, TypeScript, Vite, existing proxy/router modules, existing UI primitives.

---

## Product Boundary

P7 builds the economic layer for the existing P6 router.

P6 already decides:

- whether a Station Key is enabled
- whether it supports the endpoint/protocol
- whether it supports the model and request capabilities
- whether it is healthy enough
- whether it is in cooldown
- how to explain the route decision

P7 adds:

- normalized pricing rules
- normalized station/key balance snapshots
- cost metadata on request logs
- `cheap_first` routing policy
- low-balance avoidance
- pricing/balance explanations in the route simulator
- real pricing table data instead of mock-only UI
- small dashboard/key-pool/request-log cost summaries

P7 does not build:

- complex strategy DSL
- precise accounting or invoices
- automatic recharge
- multi-user billing
- full currency conversion
- price arbitrage optimization
- automatic perfect price discovery for every relay
- secret encryption migration
- LAN exposure

---

## Completion Standard

P7 is complete only when all gates below pass.

### Functional Gates

- [ ] Price data can be persisted in `pricing_rules` and listed in the pricing page.
- [ ] Balance data can be persisted in `balance_snapshots` and shown per Station.
- [ ] Collector snapshots can be normalized into pricing/balance rows without reading raw snapshot JSON in the router.
- [ ] Request logs include token usage and estimated cost fields when upstream returns `usage`.
- [ ] If upstream does not return `usage`, request cost is marked unknown instead of guessed.
- [ ] Router supports `cheap_first`.
- [ ] Router can skip or downgrade low-balance candidates according to policy.
- [ ] Route simulator shows price score, balance state, and final economic reason.
- [ ] Pricing page no longer relies only on mock data.
- [ ] Dashboard shows concise cost/balance summary from real stored data.
- [ ] Key Pool shows balance/cost badges without becoming a dense accounting table.
- [ ] Logs page shows cost metadata and still never stores full prompt/response/key.

### Safety Gates

- [ ] No prompt body is saved to SQLite.
- [ ] No response body is saved to SQLite.
- [ ] No full API key, cookie, token, session, or password is saved to request logs.
- [ ] Pricing/balance import keeps raw collector snapshot separate from normalized data.
- [ ] Currency/unit ambiguity is preserved as metadata instead of forced into fake CNY/USD conversion.

### Verification Gates

- [ ] `pnpm build`
- [ ] `cargo check --manifest-path .\src-tauri\Cargo.toml`
- [ ] `cargo test --manifest-path .\src-tauri\Cargo.toml --lib`
- [ ] Manual smoke: pricing rows visible after inserting or collecting pricing data.
- [ ] Manual smoke: low-balance Station is skipped or downgraded according to selected policy.
- [ ] Manual smoke: `cheap_first` selects a cheaper healthy candidate over a more expensive healthy candidate.
- [ ] Manual smoke: request logs show token/cost metadata after a real proxy request with upstream `usage`.
- [ ] Manual smoke: logs do not contain the test prompt text or full API key.

---

## File Structure

### Backend Models

- Create `src-tauri/src/models/pricing.rs`
  - Rust structs/enums for `PricingRule`, `BalanceSnapshot`, `PricingSource`, `BalanceScope`, `BalanceStatus`, `CostEstimate`, and request-log cost fields.
- Modify `src-tauri/src/models/mod.rs`
  - Export the new `pricing` module.
- Modify `src-tauri/src/models/routing.rs`
  - Add `CheapFirst` to `RoutingPolicy`.
  - Add price/balance fields to `RouteCandidateExplanation`.
  - Add optional economic controls to `RouteSimulationInput`.
- Modify `src-tauri/src/models/proxy.rs`
  - Add usage/cost fields to `RequestLog` and `CreateRequestLogInput`.

### Backend Database

- Modify `src-tauri/src/services/database.rs`
  - Add schema for `pricing_rules`, `balance_snapshots`, and request-log cost columns.
  - Add migrations for existing databases.
  - Add CRUD/list helpers for pricing and balance.
  - Add tests for persistence, migration, normalization, and request-log cost metadata.

### Backend Normalization

- Create `src-tauri/src/services/economics/mod.rs`
  - Module exports.
- Create `src-tauri/src/services/economics/pricing.rs`
  - Normalize collector snapshot pricing/group/multiplier model data into `PricingRule`.
- Create `src-tauri/src/services/economics/balance.rs`
  - Normalize balance/quota/credit fields into `BalanceSnapshot`.
- Create `src-tauri/src/services/economics/cost.rs`
  - Parse upstream OpenAI-compatible `usage` and estimate cost from normalized pricing.
- Modify `src-tauri/src/services/mod.rs`
  - Export `economics`.

### Backend Router/Proxy

- Modify `src-tauri/src/services/proxy/router.rs`
  - Score candidates with `cheap_first`.
  - Add balance rejection/downgrade reasons.
  - Add pricing/balance explanation fields.
- Modify `src-tauri/src/services/proxy/runtime.rs`
  - Parse usage from upstream JSON responses.
  - Estimate cost after successful non-streaming responses.
  - For streaming responses, record cost as unknown unless the final upstream payload exposes usage in a safely parsed event.
- Modify `src-tauri/src/services/proxy/mod.rs`
  - Extend route candidate loading if needed, keeping API key redaction intact.

### Backend Commands

- Modify `src-tauri/src/commands/mod.rs`
  - Add pricing/balance/list/update commands.
  - Extend route simulator command return type through model changes.

Suggested commands:

```text
list_pricing_rules
upsert_pricing_rule
delete_pricing_rule
list_balance_snapshots
upsert_balance_snapshot
normalize_latest_station_economics
```

### Frontend API/Types

- Create `src/lib/types/economics.ts`
  - TypeScript mirrors of pricing/balance/cost structs.
- Create `src/lib/api/economics.ts`
  - Tauri invoke wrappers.
- Modify `src/lib/types/routing.ts`
  - Include `cheap_first`, pricing score, balance status, and cost explanation fields.
- Modify `src/lib/types/proxy.ts`
  - Include request-log usage/cost fields.

### Frontend Pages

- Modify `src/features/pricing/PricingPage.tsx`
  - Replace mock-only pricing table with real `listPricingRules()` data plus empty state.
- Modify `src/features/routing/RoutingPage.tsx`
  - Add `cheap_first` to policy selection.
  - Show price/balance reasons in simulator output.
- Modify `src/features/logs/LogsPage.tsx`
  - Show token/cost metadata.
- Modify `src/features/key-pool/KeyPoolPage.tsx`
  - Add compact balance/cost badges.
- Modify `src/features/dashboard/DashboardPage.tsx`
  - Add summary cards for total known balance, low-balance Stations, today estimated cost.

### Documentation

- Create `docs/PHASE_7_PRICING_BALANCE_COST_PLAN.md`
  - User-facing phase document summarizing capabilities, non-goals, and smoke checklist.
- Modify `docs/PROJECT_PLAN.md`
  - Mark P7 as the next economic routing layer.
- Modify `docs/PRODUCT_MODEL.md`
  - Add `Pricing Rule`, `Balance Snapshot`, and `Request Cost` concepts.
- Modify `README.md`
  - Mention P7 capabilities only after implementation tasks are completed.

---

## Data Model Details

### `pricing_rules`

Create a normalized table that is route-readable and UI-readable.

Columns:

```sql
CREATE TABLE IF NOT EXISTS pricing_rules (
    id TEXT PRIMARY KEY,
    station_id TEXT NOT NULL,
    group_name TEXT,
    tier_label TEXT,
    model TEXT NOT NULL,
    input_price REAL,
    output_price REAL,
    fixed_price REAL,
    currency TEXT NOT NULL,
    unit TEXT NOT NULL,
    price_type TEXT NOT NULL,
    source TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.5,
    enabled INTEGER NOT NULL DEFAULT 1,
    note TEXT,
    collected_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE
);
```

Rules:

- `unit` must preserve meaning such as `per_1m_tokens`, `per_1k_tokens`, `credit_ratio`, or `unknown`.
- `currency` may be `CNY`, `USD`, `credit`, or `unknown`.
- `input_price`, `output_price`, and `fixed_price` can be null if unknown.
- `confidence` is a number from `0.0` to `1.0`.
- Router must ignore disabled rules.

### `balance_snapshots`

Create a normalized balance table that keeps units explicit.

Columns:

```sql
CREATE TABLE IF NOT EXISTS balance_snapshots (
    id TEXT PRIMARY KEY,
    station_id TEXT NOT NULL,
    station_key_id TEXT,
    scope TEXT NOT NULL,
    value REAL,
    currency TEXT NOT NULL,
    credit_unit TEXT,
    used_value REAL,
    total_value REAL,
    low_balance_threshold REAL,
    status TEXT NOT NULL,
    source TEXT NOT NULL,
    confidence REAL NOT NULL DEFAULT 0.5,
    collected_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
    FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE
);
```

Rules:

- `scope` is `station` or `station_key`.
- `status` is `unknown`, `normal`, `low`, or `depleted`.
- Balance usually belongs to Station.
- Key-level balance is allowed when an upstream exposes it.
- Unknown balance must not be silently treated as depleted.

### Request Log Cost Columns

Extend `request_logs`:

```sql
ALTER TABLE request_logs ADD COLUMN prompt_tokens INTEGER;
ALTER TABLE request_logs ADD COLUMN completion_tokens INTEGER;
ALTER TABLE request_logs ADD COLUMN total_tokens INTEGER;
ALTER TABLE request_logs ADD COLUMN estimated_input_cost REAL;
ALTER TABLE request_logs ADD COLUMN estimated_output_cost REAL;
ALTER TABLE request_logs ADD COLUMN estimated_total_cost REAL;
ALTER TABLE request_logs ADD COLUMN cost_currency TEXT;
ALTER TABLE request_logs ADD COLUMN pricing_rule_id TEXT;
ALTER TABLE request_logs ADD COLUMN pricing_source TEXT;
ALTER TABLE request_logs ADD COLUMN cost_status TEXT;
```

Rules:

- `cost_status` is `estimated`, `unknown_usage`, `unknown_price`, or `not_applicable`.
- Do not store prompt text.
- Do not store response text.
- Do not store full API keys.

---

## Task 1: Add Economic Model Types

**Files:**

- Create: `src-tauri/src/models/pricing.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Modify: `src-tauri/src/models/routing.rs`
- Modify: `src-tauri/src/models/proxy.rs`

- [ ] **Step 1: Write model shape tests where possible**

Add serde round-trip tests in `src-tauri/src/models/pricing.rs` under `#[cfg(test)]`.

Expected core structs:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PricingRule {
    pub id: String,
    pub station_id: String,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub model: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub price_type: String,
    pub source: String,
    pub confidence: f64,
    pub enabled: bool,
    pub note: Option<String>,
    pub collected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

Expected test:

```rust
#[test]
fn pricing_rule_serializes_camel_case() {
    let rule = PricingRule {
        id: "price-1".to_string(),
        station_id: "station-1".to_string(),
        group_name: Some("pro".to_string()),
        tier_label: None,
        model: "gpt-4o-mini".to_string(),
        input_price: Some(0.15),
        output_price: Some(0.6),
        fixed_price: None,
        currency: "USD".to_string(),
        unit: "per_1m_tokens".to_string(),
        price_type: "token".to_string(),
        source: "manual".to_string(),
        confidence: 0.9,
        enabled: true,
        note: None,
        collected_at: Some("1000".to_string()),
        created_at: "1000".to_string(),
        updated_at: "1000".to_string(),
    };

    let json = serde_json::to_value(rule).expect("json");
    assert_eq!(json["stationId"], "station-1");
    assert_eq!(json["inputPrice"], 0.15);
    assert_eq!(json["priceType"], "token");
}
```

- [ ] **Step 2: Run the model test and verify it fails before implementation**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_rule_serializes_camel_case --lib
```

Expected: fail because `PricingRule` does not exist.

- [ ] **Step 3: Implement `pricing.rs`**

Add:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PricingRule {
    pub id: String,
    pub station_id: String,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub model: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub price_type: String,
    pub source: String,
    pub confidence: f64,
    pub enabled: bool,
    pub note: Option<String>,
    pub collected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSnapshot {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub scope: String,
    pub value: Option<f64>,
    pub currency: String,
    pub credit_unit: Option<String>,
    pub used_value: Option<f64>,
    pub total_value: Option<f64>,
    pub low_balance_threshold: Option<f64>,
    pub status: String,
    pub source: String,
    pub confidence: f64,
    pub collected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpsertPricingRuleInput {
    pub id: Option<String>,
    pub station_id: String,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub model: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub price_type: String,
    pub source: String,
    pub confidence: f64,
    pub enabled: bool,
    pub note: Option<String>,
    pub collected_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpsertBalanceSnapshotInput {
    pub id: Option<String>,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub scope: String,
    pub value: Option<f64>,
    pub currency: String,
    pub credit_unit: Option<String>,
    pub used_value: Option<f64>,
    pub total_value: Option<f64>,
    pub low_balance_threshold: Option<f64>,
    pub status: String,
    pub source: String,
    pub confidence: f64,
    pub collected_at: Option<String>,
}
```

- [ ] **Step 4: Export the module**

Modify `src-tauri/src/models/mod.rs`:

```rust
pub mod pricing;
```

- [ ] **Step 5: Extend routing policy**

Modify `RoutingPolicy` in `src-tauri/src/models/routing.rs`:

```rust
pub enum RoutingPolicy {
    PriorityFallback,
    StableFirst,
    BackupOnly,
    CheapFirst,
}
```

Extend `RouteCandidateExplanation` with:

```rust
pub pricing_rule_id: Option<String>,
pub estimated_input_price: Option<f64>,
pub estimated_output_price: Option<f64>,
pub price_currency: Option<String>,
pub balance_status: Option<String>,
pub balance_value: Option<f64>,
pub economic_reasons: Vec<String>,
```

- [ ] **Step 6: Extend request-log models**

Modify `RequestLog` and `CreateRequestLogInput` in `src-tauri/src/models/proxy.rs`:

```rust
pub prompt_tokens: Option<i64>,
pub completion_tokens: Option<i64>,
pub total_tokens: Option<i64>,
pub estimated_input_cost: Option<f64>,
pub estimated_output_cost: Option<f64>,
pub estimated_total_cost: Option<f64>,
pub cost_currency: Option<String>,
pub pricing_rule_id: Option<String>,
pub pricing_source: Option<String>,
pub cost_status: Option<String>,
```

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml pricing --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: model tests pass; compile errors from database/router call sites are acceptable until Task 2/4 if this is committed in isolation only after fixing call sites.

- [ ] **Step 8: Commit**

Commit only after compile is clean:

```powershell
git add -- src-tauri/src/models/pricing.rs src-tauri/src/models/mod.rs src-tauri/src/models/routing.rs src-tauri/src/models/proxy.rs
git commit -m "feat: add pricing and balance route models"
```

---

## Task 2: Add Pricing and Balance Persistence

**Files:**

- Modify: `src-tauri/src/services/database.rs`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write failing persistence tests**

Add tests:

```rust
#[test]
fn pricing_rule_round_trip() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "pricing-rule");

    let saved = database
        .upsert_pricing_rule(UpsertPricingRuleInput {
            id: None,
            station_id: station.id.clone(),
            group_name: Some("pro".to_string()),
            tier_label: Some("tier-a".to_string()),
            model: "gpt-4o-mini".to_string(),
            input_price: Some(0.15),
            output_price: Some(0.6),
            fixed_price: None,
            currency: "USD".to_string(),
            unit: "per_1m_tokens".to_string(),
            price_type: "token".to_string(),
            source: "manual".to_string(),
            confidence: 0.9,
            enabled: true,
            note: Some("manual override".to_string()),
            collected_at: Some("1000".to_string()),
        })
        .expect("save");

    let rows = database.list_pricing_rules().expect("pricing rules");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, saved.id);
    assert_eq!(rows[0].station_id, station.id);
    assert_eq!(rows[0].model, "gpt-4o-mini");
    assert_eq!(rows[0].input_price, Some(0.15));
}

#[test]
fn balance_snapshot_round_trip() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "balance-snapshot");

    let saved = database
        .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
            id: None,
            station_id: station.id.clone(),
            station_key_id: None,
            scope: "station".to_string(),
            value: Some(12.5),
            currency: "CNY".to_string(),
            credit_unit: None,
            used_value: None,
            total_value: None,
            low_balance_threshold: Some(5.0),
            status: "normal".to_string(),
            source: "collector".to_string(),
            confidence: 0.8,
            collected_at: Some("1000".to_string()),
        })
        .expect("save");

    let rows = database.list_balance_snapshots().expect("balances");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, saved.id);
    assert_eq!(rows[0].value, Some(12.5));
    assert_eq!(rows[0].status, "normal");
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_rule_round_trip balance_snapshot_round_trip --lib
```

Expected: fail because database methods/tables do not exist.

- [ ] **Step 3: Add schema**

In `initialize_database`, add the `pricing_rules` and `balance_snapshots` `CREATE TABLE IF NOT EXISTS` statements from the Data Model section.

Add indexes:

```sql
CREATE INDEX IF NOT EXISTS idx_pricing_rules_station_model
    ON pricing_rules(station_id, model);

CREATE INDEX IF NOT EXISTS idx_pricing_rules_model_enabled
    ON pricing_rules(model, enabled);

CREATE INDEX IF NOT EXISTS idx_balance_snapshots_station
    ON balance_snapshots(station_id, updated_at DESC);
```

- [ ] **Step 4: Add request-log cost migrations**

Create a helper modeled after the existing request-log route metadata migration:

```rust
fn migrate_request_log_cost_columns(connection: &Connection) -> rusqlite::Result<()> {
    let columns = table_columns(connection, "request_logs")?;
    add_column_if_missing(connection, &columns, "request_logs", "prompt_tokens INTEGER")?;
    add_column_if_missing(connection, &columns, "request_logs", "completion_tokens INTEGER")?;
    add_column_if_missing(connection, &columns, "request_logs", "total_tokens INTEGER")?;
    add_column_if_missing(connection, &columns, "request_logs", "estimated_input_cost REAL")?;
    add_column_if_missing(connection, &columns, "request_logs", "estimated_output_cost REAL")?;
    add_column_if_missing(connection, &columns, "request_logs", "estimated_total_cost REAL")?;
    add_column_if_missing(connection, &columns, "request_logs", "cost_currency TEXT")?;
    add_column_if_missing(connection, &columns, "request_logs", "pricing_rule_id TEXT")?;
    add_column_if_missing(connection, &columns, "request_logs", "pricing_source TEXT")?;
    add_column_if_missing(connection, &columns, "request_logs", "cost_status TEXT")?;
    Ok(())
}
```

If `table_columns` and `add_column_if_missing` do not exist, add focused private helpers:

```rust
fn table_columns(connection: &Connection, table_name: &str) -> rusqlite::Result<Vec<String>> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table_name})"))?;
    statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()
}

fn add_column_if_missing(
    connection: &Connection,
    columns: &[String],
    table_name: &str,
    column_sql: &str,
) -> rusqlite::Result<()> {
    let column_name = column_sql
        .split_whitespace()
        .next()
        .expect("column name");
    if !columns.iter().any(|column| column == column_name) {
        connection.execute(&format!("ALTER TABLE {table_name} ADD COLUMN {column_sql}"), [])?;
    }
    Ok(())
}
```

- [ ] **Step 5: Add database methods**

Add public methods to `impl AppDatabase`:

```rust
pub fn list_pricing_rules(&self) -> Result<Vec<PricingRule>, String>;
pub fn upsert_pricing_rule(&self, input: UpsertPricingRuleInput) -> Result<PricingRule, String>;
pub fn delete_pricing_rule(&self, id: String) -> Result<(), String>;
pub fn list_balance_snapshots(&self) -> Result<Vec<BalanceSnapshot>, String>;
pub fn upsert_balance_snapshot(&self, input: UpsertBalanceSnapshotInput) -> Result<BalanceSnapshot, String>;
```

Validation rules:

- `station_id` must exist.
- `model` must be non-empty for pricing.
- `confidence` must be clamped to `0.0..=1.0`.
- `scope` must be `station` or `station_key`.
- `status` must be `unknown`, `normal`, `low`, or `depleted`.
- `currency` must be preserved as a trimmed string; empty becomes `unknown`.

- [ ] **Step 6: Extend request-log insert and read**

Update `create_request_log_from_connection`, `row_to_request_log`, `list_request_logs_from_connection`, and `request_log_by_id` to include cost columns.

When inserting old callers that provide `None`, the database row must still save.

- [ ] **Step 7: Run persistence tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_rule_round_trip balance_snapshot_round_trip --lib
cargo test --manifest-path .\src-tauri\Cargo.toml request_log --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: all pass with no new warnings.

- [ ] **Step 8: Commit**

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/models/proxy.rs src-tauri/src/models/pricing.rs
git commit -m "feat: persist pricing balance and request costs"
```

---

## Task 3: Add Economics Normalization Services

**Files:**

- Create: `src-tauri/src/services/economics/mod.rs`
- Create: `src-tauri/src/services/economics/pricing.rs`
- Create: `src-tauri/src/services/economics/balance.rs`
- Create: `src-tauri/src/services/economics/cost.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Test: new modules under `#[cfg(test)]`

- [ ] **Step 1: Write pricing normalization test**

In `pricing.rs`:

```rust
#[test]
fn normalizes_simple_model_price_json() {
    let json = serde_json::json!({
        "models": [
            {
                "model": "gpt-4o-mini",
                "input_price": 0.15,
                "output_price": 0.60,
                "currency": "USD",
                "unit": "per_1m_tokens"
            }
        ]
    });

    let rows = normalize_pricing_from_snapshot("station-1", &json, "collector", "1000");

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].model, "gpt-4o-mini");
    assert_eq!(rows[0].input_price, Some(0.15));
    assert_eq!(rows[0].output_price, Some(0.60));
    assert_eq!(rows[0].source, "collector");
}
```

- [ ] **Step 2: Write balance normalization test**

In `balance.rs`:

```rust
#[test]
fn normalizes_balance_fields_without_forcing_currency_conversion() {
    let json = serde_json::json!({
        "balance": 18.5,
        "currency": "CNY"
    });

    let row = normalize_balance_from_snapshot("station-1", &json, "collector", "1000")
        .expect("balance");

    assert_eq!(row.station_id, "station-1");
    assert_eq!(row.scope, "station");
    assert_eq!(row.value, Some(18.5));
    assert_eq!(row.currency, "CNY");
    assert_eq!(row.status, "normal");
}
```

- [ ] **Step 3: Write cost estimate test**

In `cost.rs`:

```rust
#[test]
fn estimates_cost_from_openai_usage_and_per_1m_prices() {
    let usage = serde_json::json!({
        "prompt_tokens": 1000,
        "completion_tokens": 500,
        "total_tokens": 1500
    });
    let rule = PricingRule {
        id: "price-1".to_string(),
        station_id: "station-1".to_string(),
        group_name: None,
        tier_label: None,
        model: "gpt-4o-mini".to_string(),
        input_price: Some(0.15),
        output_price: Some(0.60),
        fixed_price: None,
        currency: "USD".to_string(),
        unit: "per_1m_tokens".to_string(),
        price_type: "token".to_string(),
        source: "manual".to_string(),
        confidence: 1.0,
        enabled: true,
        note: None,
        collected_at: None,
        created_at: "1000".to_string(),
        updated_at: "1000".to_string(),
    };

    let cost = estimate_cost_from_usage(&usage, Some(&rule));

    assert_eq!(cost.prompt_tokens, Some(1000));
    assert_eq!(cost.completion_tokens, Some(500));
    assert_eq!(cost.total_tokens, Some(1500));
    assert_eq!(cost.estimated_input_cost, Some(0.00015));
    assert_eq!(cost.estimated_output_cost, Some(0.0003));
    assert_eq!(cost.estimated_total_cost, Some(0.00045));
    assert_eq!(cost.cost_status, "estimated");
}
```

- [ ] **Step 4: Run tests to verify failure**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml economics --lib
```

Expected: fail because modules/functions do not exist.

- [ ] **Step 5: Implement normalization module exports**

`src-tauri/src/services/economics/mod.rs`:

```rust
pub mod balance;
pub mod cost;
pub mod pricing;
```

`src-tauri/src/services/mod.rs`:

```rust
pub mod economics;
```

- [ ] **Step 6: Implement pricing normalization**

Support the first safe shapes:

- `models[].model`
- `models[].input_price`
- `models[].output_price`
- `models[].fixed_price`
- `models[].currency`
- `models[].unit`
- `models[].group_name`
- `models[].tier_label`
- top-level object keyed by model name with price object

Rules:

- ignore entries without a model name
- confidence `0.8` for direct normalized fields
- confidence `0.5` for inferred multiplier/ratio fields
- currency defaults to `unknown`
- unit defaults to `unknown`

- [ ] **Step 7: Implement balance normalization**

Support fields:

- `balance`
- `quota`
- `credit`
- `remaining`
- `remain`
- `used`
- `usage`
- `total`
- `currency`

Status rule:

```rust
fn balance_status(value: Option<f64>, threshold: Option<f64>) -> String {
    match value {
        Some(value) if value <= 0.0 => "depleted".to_string(),
        Some(value) if threshold.is_some_and(|threshold| value <= threshold) => "low".to_string(),
        Some(_) => "normal".to_string(),
        None => "unknown".to_string(),
    }
}
```

- [ ] **Step 8: Implement cost estimation**

Implement:

```rust
pub fn estimate_cost_from_usage(
    usage: &serde_json::Value,
    pricing_rule: Option<&PricingRule>,
) -> RequestCostEstimate
```

Rules:

- parse `prompt_tokens`, `completion_tokens`, `total_tokens`
- if usage missing, return `cost_status = "unknown_usage"`
- if pricing missing, return `cost_status = "unknown_price"` with token counts preserved
- for `per_1m_tokens`, divide token count by `1_000_000`
- for `per_1k_tokens`, divide token count by `1_000`
- include fixed price when present
- do not inspect prompt or response body content beyond `usage`

- [ ] **Step 9: Run tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml economics --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

- [ ] **Step 10: Commit**

```powershell
git add -- src-tauri/src/services/economics src-tauri/src/services/mod.rs
git commit -m "feat: normalize pricing balance and request cost"
```

---

## Task 4: Connect Collector Snapshots to Normalized Economics

**Files:**

- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write failing integration test**

Add:

```rust
#[test]
fn normalizes_latest_collector_snapshot_into_pricing_and_balance() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "economics-normalize");

    database
        .create_collector_snapshot(CreateCollectorSnapshotInput {
            station_id: station.id.clone(),
            source: "collector".to_string(),
            status: "success".to_string(),
            fetched_at: "1000".to_string(),
            summary_json: "{}".to_string(),
            normalized_json: serde_json::json!({
                "balance": 20.0,
                "currency": "CNY",
                "models": [
                    {
                        "model": "gpt-4o-mini",
                        "input_price": 0.15,
                        "output_price": 0.6,
                        "currency": "USD",
                        "unit": "per_1m_tokens"
                    }
                ]
            })
            .to_string(),
            raw_json_redacted: None,
            error_message: None,
        })
        .expect("snapshot");

    let result = database
        .normalize_latest_station_economics(station.id.clone())
        .expect("normalize");

    assert_eq!(result.pricing_rules_created, 1);
    assert_eq!(result.balance_snapshots_created, 1);
}
```

- [ ] **Step 2: Run test to verify failure**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml normalizes_latest_collector_snapshot_into_pricing_and_balance --lib
```

Expected: fail because `normalize_latest_station_economics` does not exist.

- [ ] **Step 3: Add result model**

In `pricing.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizeStationEconomicsResult {
    pub station_id: String,
    pub pricing_rules_created: usize,
    pub balance_snapshots_created: usize,
    pub message: String,
}
```

- [ ] **Step 4: Implement database method**

Add:

```rust
pub fn normalize_latest_station_economics(
    &self,
    station_id: String,
) -> Result<NormalizeStationEconomicsResult, String>
```

Behavior:

- validate Station exists
- load latest successful collector snapshot by `fetched_at DESC, created_at DESC`
- parse `normalized_json`
- run pricing and balance normalizers
- upsert produced rows
- return counts
- if no snapshot exists, return a readable error: `没有可归一化的采集快照`

- [ ] **Step 5: Add Tauri commands**

In `src-tauri/src/commands/mod.rs`:

```rust
#[tauri::command]
pub fn normalize_latest_station_economics(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<NormalizeStationEconomicsResult, String> {
    database.normalize_latest_station_economics(station_id)
}
```

Add the command to the Tauri invoke handler list.

- [ ] **Step 6: Run tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml normalize_latest_station_economics --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

- [ ] **Step 7: Commit**

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/models/pricing.rs
git commit -m "feat: normalize collector economics"
```

---

## Task 5: Add Cheap-First Selector and Low-Balance Avoidance

**Files:**

- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/routing.rs`
- Test: `src-tauri/src/services/proxy/router.rs`

- [ ] **Step 1: Write `cheap_first` selector test**

Add to `router.rs` tests:

```rust
#[test]
fn selector_cheap_first_prefers_lower_output_price_when_health_is_usable() {
    let request = route_request(
        RouteEndpointKind::ChatCompletions,
        Some("gpt-4o-mini"),
        false,
        RoutingPolicy::CheapFirst,
    );
    let candidates = vec![
        rich_candidate_with_economics("expensive", 0, 1.20, "normal"),
        rich_candidate_with_economics("cheap", 10, 0.60, "normal"),
    ];

    let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

    assert_eq!(selected.accepted[0].candidate.station_key_id, "cheap");
    assert!(selected.explanations.iter().any(|item| {
        item.station_key_id == "cheap"
            && item.economic_reasons.iter().any(|reason| reason.contains("lower estimated price"))
    }));
}
```

- [ ] **Step 2: Write low-balance skip test**

```rust
#[test]
fn selector_skips_depleted_balance_candidate() {
    let request = route_request(
        RouteEndpointKind::ChatCompletions,
        Some("gpt-4o-mini"),
        false,
        RoutingPolicy::CheapFirst,
    );
    let candidates = vec![
        rich_candidate_with_economics("depleted", 0, 0.10, "depleted"),
        rich_candidate_with_economics("usable", 10, 0.60, "normal"),
    ];

    let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

    assert_eq!(selected.accepted[0].candidate.station_key_id, "usable");
    assert!(selected.explanations.iter().any(|item| {
        item.station_key_id == "depleted"
            && item.rejection_reasons.iter().any(|reason| reason.contains("balance depleted"))
    }));
}
```

- [ ] **Step 3: Run tests to verify failure**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml selector_cheap_first selector_skips_depleted --lib
```

Expected: fail because router candidate economics do not exist.

- [ ] **Step 4: Extend rich route candidate**

In routing model or router-local struct:

```rust
pub struct CandidateEconomics {
    pub pricing_rule_id: Option<String>,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub currency: Option<String>,
    pub balance_status: Option<String>,
    pub balance_value: Option<f64>,
}
```

Add:

```rust
pub economics: Option<CandidateEconomics>,
```

to `RichRouteCandidate`.

- [ ] **Step 5: Load economics in database candidate query**

In `list_rich_route_candidates`, join:

- latest enabled pricing rule by station/model when model is known
- latest balance snapshot by station

If the existing function cannot know the model, add a separate function:

```rust
pub fn list_rich_route_candidates_for_model(
    &self,
    model: Option<&str>,
) -> Result<Vec<RichRouteCandidate>, String>
```

Runtime and simulator should use the model-aware version.

- [ ] **Step 6: Implement scoring**

For `CheapFirst`:

- reject `balance_status == "depleted"`
- add a high score penalty for `balance_status == "low"`
- add high score penalty for unknown price
- prefer lower `output_price` first, then lower `input_price`
- preserve health/cooldown rejections from P6

Suggested scoring:

```rust
if matches!(request.policy, RoutingPolicy::CheapFirst) {
    score += price_score(candidate.economics.as_ref());
    score += balance_penalty(candidate.economics.as_ref());
    score += health_penalty(candidate.health.as_ref());
}
```

Where:

```rust
fn price_score(economics: Option<&CandidateEconomics>) -> i64 {
    let Some(economics) = economics else {
        return 1_000_000;
    };
    let output = economics.output_price.unwrap_or(999_999.0);
    let input = economics.input_price.unwrap_or(999_999.0);
    ((output * 10_000.0) + (input * 1_000.0)) as i64
}
```

- [ ] **Step 7: Run router tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml selector_ --lib
cargo test --manifest-path .\src-tauri\Cargo.toml proxy::router --lib
```

- [ ] **Step 8: Commit**

```powershell
git add -- src-tauri/src/services/proxy/router.rs src-tauri/src/services/database.rs src-tauri/src/models/routing.rs
git commit -m "feat: route by price and balance"
```

---

## Task 6: Record Usage and Cost in Proxy Request Logs

**Files:**

- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/proxy.rs`
- Test: `src-tauri/src/services/proxy/runtime.rs`

- [ ] **Step 1: Write non-streaming cost log test**

In runtime tests, add a fake upstream JSON response:

```rust
#[test]
fn successful_non_streaming_request_records_usage_cost_metadata() {
    let response = serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "choices": [],
        "usage": {
            "prompt_tokens": 1000,
            "completion_tokens": 500,
            "total_tokens": 1500
        }
    });

    let cost = extract_usage_cost_for_test(response, pricing_rule_for_test());

    assert_eq!(cost.prompt_tokens, Some(1000));
    assert_eq!(cost.completion_tokens, Some(500));
    assert_eq!(cost.total_tokens, Some(1500));
    assert_eq!(cost.cost_status, "estimated");
}
```

- [ ] **Step 2: Write missing usage test**

```rust
#[test]
fn response_without_usage_records_unknown_usage() {
    let response = serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "choices": []
    });

    let cost = extract_usage_cost_for_test(response, pricing_rule_for_test());

    assert_eq!(cost.prompt_tokens, None);
    assert_eq!(cost.estimated_total_cost, None);
    assert_eq!(cost.cost_status, "unknown_usage");
}
```

- [ ] **Step 3: Run tests to verify failure**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml cost_metadata --lib
```

Expected: fail because helper/runtime parsing does not exist.

- [ ] **Step 4: Parse usage only from upstream JSON metadata**

Behavior:

- For non-streaming JSON responses, parse `usage`.
- Do not persist request body.
- Do not persist response body.
- For streamed responses, record `cost_status = "unknown_usage"` unless a final event contains usage and can be parsed without storing stream contents.

- [ ] **Step 5: Attach cost data to request logs**

When `CreateRequestLogInput` is built after a proxy request:

```rust
prompt_tokens: cost.prompt_tokens,
completion_tokens: cost.completion_tokens,
total_tokens: cost.total_tokens,
estimated_input_cost: cost.estimated_input_cost,
estimated_output_cost: cost.estimated_output_cost,
estimated_total_cost: cost.estimated_total_cost,
cost_currency: cost.cost_currency,
pricing_rule_id: cost.pricing_rule_id,
pricing_source: cost.pricing_source,
cost_status: Some(cost.cost_status),
```

- [ ] **Step 6: Run runtime tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml runtime --lib
cargo test --manifest-path .\src-tauri\Cargo.toml request_log --lib
```

- [ ] **Step 7: Commit**

```powershell
git add -- src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/database.rs src-tauri/src/models/proxy.rs
git commit -m "feat: record proxy usage cost metadata"
```

---

## Task 7: Add Backend Commands and Frontend API Types

**Files:**

- Modify: `src-tauri/src/commands/mod.rs`
- Create: `src/lib/types/economics.ts`
- Create: `src/lib/api/economics.ts`
- Modify: `src/lib/types/routing.ts`
- Modify: `src/lib/types/proxy.ts`

- [ ] **Step 1: Add TypeScript types**

`src/lib/types/economics.ts`:

```ts
export interface PricingRule {
  id: string;
  stationId: string;
  groupName?: string | null;
  tierLabel?: string | null;
  model: string;
  inputPrice?: number | null;
  outputPrice?: number | null;
  fixedPrice?: number | null;
  currency: string;
  unit: string;
  priceType: string;
  source: string;
  confidence: number;
  enabled: boolean;
  note?: string | null;
  collectedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface BalanceSnapshot {
  id: string;
  stationId: string;
  stationKeyId?: string | null;
  scope: "station" | "station_key";
  value?: number | null;
  currency: string;
  creditUnit?: string | null;
  usedValue?: number | null;
  totalValue?: number | null;
  lowBalanceThreshold?: number | null;
  status: "unknown" | "normal" | "low" | "depleted";
  source: string;
  confidence: number;
  collectedAt?: string | null;
  createdAt: string;
  updatedAt: string;
}
```

- [ ] **Step 2: Add API wrappers**

`src/lib/api/economics.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type { BalanceSnapshot, PricingRule } from "@/lib/types/economics";

export function listPricingRules(): Promise<PricingRule[]> {
  return invoke("list_pricing_rules");
}

export function listBalanceSnapshots(): Promise<BalanceSnapshot[]> {
  return invoke("list_balance_snapshots");
}

export function normalizeLatestStationEconomics(stationId: string) {
  return invoke("normalize_latest_station_economics", { stationId });
}
```

- [ ] **Step 3: Add Tauri command functions**

In `commands/mod.rs`, add:

```rust
#[tauri::command]
pub fn list_pricing_rules(database: State<'_, AppDatabase>) -> Result<Vec<PricingRule>, String> {
    database.list_pricing_rules()
}

#[tauri::command]
pub fn list_balance_snapshots(database: State<'_, AppDatabase>) -> Result<Vec<BalanceSnapshot>, String> {
    database.list_balance_snapshots()
}
```

Register in invoke handler.

- [ ] **Step 4: Extend routing/proxy frontend types**

Add `cheap_first` to policy union.

Add cost fields to request log type:

```ts
promptTokens?: number | null;
completionTokens?: number | null;
totalTokens?: number | null;
estimatedInputCost?: number | null;
estimatedOutputCost?: number | null;
estimatedTotalCost?: number | null;
costCurrency?: string | null;
pricingRuleId?: string | null;
pricingSource?: string | null;
costStatus?: "estimated" | "unknown_usage" | "unknown_price" | "not_applicable" | string | null;
```

- [ ] **Step 5: Run verification**

```powershell
pnpm build
cargo check --manifest-path .\src-tauri\Cargo.toml
```

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/commands/mod.rs src/lib/types/economics.ts src/lib/api/economics.ts src/lib/types/routing.ts src/lib/types/proxy.ts
git commit -m "feat: expose pricing balance APIs"
```

---

## Task 8: Replace Pricing Page Mock With Real Pricing Data

**Files:**

- Modify: `src/features/pricing/PricingPage.tsx`

- [ ] **Step 1: Add loading real pricing rows**

Use:

```ts
const [pricingRules, setPricingRules] = useState<PricingRule[]>([]);
const [isLoading, setIsLoading] = useState(false);
const [error, setError] = useState<string | null>(null);
```

Load with:

```ts
const loadPricing = useCallback(async () => {
  setIsLoading(true);
  setError(null);
  try {
    setPricingRules(await listPricingRules());
  } catch (error) {
    setError(error instanceof Error ? error.message : String(error));
  } finally {
    setIsLoading(false);
  }
}, []);
```

- [ ] **Step 2: Keep mock only as empty demo fallback label**

If `pricingRules.length === 0`, show an empty state:

```text
暂无真实价格数据。请先在信息采集页采集站点信息，或在价格表中手动添加价格规则。
```

Do not label mock rows as real data.

- [ ] **Step 3: Render columns**

Minimum columns:

- model
- station
- group/tier
- input price
- output price
- currency/unit
- source
- confidence
- updated_at

- [ ] **Step 4: Add filters**

Add local filters:

- search by model
- filter by station
- filter by source

Filtering must not mutate persisted order or persisted rows.

- [ ] **Step 5: Run frontend build**

```powershell
pnpm build
```

- [ ] **Step 6: Commit**

```powershell
git add -- src/features/pricing/PricingPage.tsx
git commit -m "feat: show real pricing data"
```

---

## Task 9: Show Economics in Routing Simulator

**Files:**

- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/lib/types/routing.ts`

- [ ] **Step 1: Add `cheap_first` policy option**

Display label:

```text
便宜优先
```

Description:

```text
在能力、健康和余额可用的前提下，优先选择价格更低的 Key。
```

- [ ] **Step 2: Show economic reasons**

For each candidate, render:

- price badge
- balance status badge
- economic reasons
- rejection reasons

Example text:

```text
价格：USD 0.60 / 1M output
余额：normal · 20.00 CNY
经济原因：lower estimated price, balance normal
```

- [ ] **Step 3: Ensure no full API key appears**

Simulator candidate display must use key name and station name only. If masked key is already available, display masked key only.

- [ ] **Step 4: Run frontend build**

```powershell
pnpm build
```

- [ ] **Step 5: Commit**

```powershell
git add -- src/features/routing/RoutingPage.tsx src/lib/types/routing.ts
git commit -m "feat: explain pricing balance in routing"
```

---

## Task 10: Show Cost Metadata in Logs, Key Pool, and Dashboard

**Files:**

- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`

- [ ] **Step 1: Logs page cost columns**

Add compact fields:

- tokens
- estimated total cost
- currency
- cost status

Display:

```text
1,500 tokens · USD 0.00045 · estimated
```

When unknown:

```text
成本未知：上游未返回 usage
```

- [ ] **Step 2: Key Pool badges**

Add small badges:

- Station balance status
- latest known balance
- recent estimated spend if available from logs

Do not add wide accounting columns to the row. Keep it scan-friendly.

- [ ] **Step 3: Dashboard summary**

Add concise cards:

- known total balance by currency
- low-balance station count
- today's estimated cost
- highest-cost model today

If data is missing:

```text
暂无成本数据
```

- [ ] **Step 4: Run frontend build**

```powershell
pnpm build
```

- [ ] **Step 5: Commit**

```powershell
git add -- src/features/logs/LogsPage.tsx src/features/key-pool/KeyPoolPage.tsx src/features/dashboard/DashboardPage.tsx
git commit -m "feat: surface balance and request cost"
```

---

## Task 11: Update Documentation

**Files:**

- Create: `docs/PHASE_7_PRICING_BALANCE_COST_PLAN.md`
- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/PRODUCT_MODEL.md`
- Modify: `README.md`

- [ ] **Step 1: Write phase document**

Include:

- P7 goal
- what belongs to Station
- what belongs to Station Key
- what belongs to Pricing Rule
- what belongs to Request Cost
- supported policies
- non-goals
- smoke checklist

- [ ] **Step 2: Update product model**

Add:

```text
Pricing Rule = normalized price rule for station/group/model.
Balance Snapshot = normalized account/key balance state with explicit unit.
Request Cost = per-request usage and estimated cost metadata derived from upstream usage.
```

- [ ] **Step 3: Update project plan**

Mark P7 as:

```text
能力 + 健康 + 成本 + 余额 routing layer.
```

- [ ] **Step 4: Update README**

Mention P7 only as completed after the implementation is verified. If implementation is partial, write:

```text
P7 in progress: pricing/balance/cost routing layer.
```

- [ ] **Step 5: Commit**

```powershell
git add -- docs/PHASE_7_PRICING_BALANCE_COST_PLAN.md docs/PROJECT_PLAN.md docs/PRODUCT_MODEL.md README.md
git commit -m "docs: document p7 pricing balance cost layer"
```

---

## Task 12: End-to-End Verification

**Files:**

- No source changes unless verification exposes a bug.

- [ ] **Step 1: Run full automated checks**

```powershell
pnpm build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected:

- all commands exit 0
- no new warnings introduced by P7

- [ ] **Step 2: Start app**

```powershell
pnpm tauri:dev
```

- [ ] **Step 3: Prepare test data**

Use UI only; do not write secrets into shell history.

Minimum:

- one Station with a working test key
- one Station with a second working or intentionally bad key
- one pricing rule for `gpt-4o-mini` or the actual test model
- one cheaper pricing rule on another Station/Key
- one balance snapshot marked `normal`
- one balance snapshot marked `low` or `depleted`

- [ ] **Step 4: Verify pricing page**

Expected:

- real pricing rule rows appear
- model search works
- station/source filters work
- empty state is clear when no rows exist

- [ ] **Step 5: Verify route simulator**

Simulate:

```text
endpoint = /v1/chat/completions
model = actual test model
stream = false
policy = cheap_first
```

Expected:

- candidates show price/balance explanations
- depleted candidate is rejected
- cheap healthy candidate is selected
- selected reason is readable

- [ ] **Step 6: Verify real proxy route**

Start local proxy and call:

```powershell
$body = @'
{
  "model": "<actual-test-model>",
  "messages": [
    { "role": "user", "content": "只回复 pong" }
  ],
  "stream": false
}
'@

curl.exe "http://127.0.0.1:<port>/v1/chat/completions" `
  -H "Content-Type: application/json" `
  -d $body
```

Expected:

- request succeeds
- selected key matches simulator for the same policy
- request log shows route policy and economic reason
- prompt text is not stored in logs

- [ ] **Step 7: Verify cost log**

Open Logs page.

Expected:

- if upstream returned usage, token counts and estimated cost are shown
- if upstream did not return usage, cost status says unknown
- no prompt body
- no response body
- no full API key

- [ ] **Step 8: Verify low-balance behavior**

Mark the cheapest Station as depleted.

Run simulator and real proxy again.

Expected:

- depleted cheapest candidate is rejected
- next usable candidate is selected
- rejected reason says balance depleted

- [ ] **Step 9: Verify dashboard/key pool**

Expected:

- Dashboard shows known balance and estimated spend summary.
- Key Pool shows compact balance/cost badges.
- UI stays dense and scannable.

- [ ] **Step 10: Final status**

Run:

```powershell
git status --short
git log --oneline -8
```

Report:

- files changed
- commits made
- verification commands
- manual smoke outcome
- known limitations
- whether P7 is ready to push

---

## Known Implementation Risks

- Pricing units are messy. Preserve units instead of pretending every value is comparable.
- Some relays expose quota points rather than currency. Store `currency = credit` or `unknown`; do not force exchange rates.
- Streaming responses may not expose usage. Mark cost unknown unless usage is safely available.
- Price-based routing must not override capability, health, cooldown, or depleted balance filters.
- Request logs must stay metadata-only.
- Existing dirty UI worktree files must be protected during implementation; use exact path staging.

---

## Suggested Commit Sequence

1. `feat: add pricing and balance route models`
2. `feat: persist pricing balance and request costs`
3. `feat: normalize pricing balance and request cost`
4. `feat: normalize collector economics`
5. `feat: route by price and balance`
6. `feat: record proxy usage cost metadata`
7. `feat: expose pricing balance APIs`
8. `feat: show real pricing data`
9. `feat: explain pricing balance in routing`
10. `feat: surface balance and request cost`
11. `docs: document p7 pricing balance cost layer`

Use exact staging. Do not use `git add .` or `git add -A`.

---

## P7 Pass/Fail Rubric

P7 passes when the app can answer these with real stored data:

```text
这个模型在哪些 enabled Station Key 上可用？
这些候选 Key 哪个有可用价格？
哪个候选更便宜？
哪个候选所属 Station 余额正常、低、耗尽或未知？
cheap_first 为什么选择这把 key？
低余额或耗尽时为什么跳过/降权？
这次请求上游返回了多少 token？
这次请求估算花费是多少？
如果 usage 或 pricing 不存在，为什么成本未知？
```

P7 fails if any of these are true:

- pricing page still only shows mock rows
- router cannot select by `cheap_first`
- low/depleted balance has no effect on routing
- request logs store prompt text
- request logs store response text
- request logs expose full API key
- cost is guessed when upstream does not return usage
- simulator explanation and real proxy behavior disagree for the same candidate set
- balance/currency/unit ambiguity is hidden from the user

