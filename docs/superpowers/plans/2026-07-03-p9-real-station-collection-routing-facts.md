# P9 Real Station Collection and Routing Facts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade Relay Pool Desktop into a real Sub2API/NewAPI/OpenAI-compatible asset console with durable collection facts, group/rate history, price and balance-aware routing, actionable change events, and UI pages that consume stable facts instead of raw collector snapshots.

**Architecture:** Build P9 from the bottom up: first extend SQLite and shared types, then add collector facts and adapter dispatch, then implement Sub2API and NewAPI adapters, then wire facts into pricing/routing/change events, and only then upgrade UI pages. `collector_snapshots` remains a redacted debug/audit artifact; business pages and routing must consume `collector_runs`, `station_group_bindings`, `group_rate_records`, `balance_snapshots`, `pricing_rules`, and structured route economics.

**Tech Stack:** Tauri 2, Rust, rusqlite, serde, serde_json, ureq, existing SecretManager/redaction utilities, React, TypeScript, Vite, Tailwind CSS, existing command/API wrappers, existing desktop UI primitives.

---

## Source Spec

Primary spec:

- `docs/superpowers/specs/2026-07-03-p9-real-station-collection-routing-facts-design.md`

This plan intentionally tracks that spec. If implementation discovers a conflict, update the spec first, then update this plan in the same docs commit.

---

## Execution Rules

- Start every implementation session with:

```powershell
git status --short -- .
git diff --cached --name-only
git branch --show-current
```

- Do not use `git add .`, `git add -A`, or `git commit -a`.
- Stage exact paths only.
- Preserve user changes. If an edited file contains unrelated user changes, stage only the intended hunks.
- Keep commits by concern. Do not combine schema, adapter, routing, and UI in one commit.
- Use `git -c gc.auto=0 commit -m "<message>"` if Git auto-maintenance hangs.
- After Rust/schema tasks, run at least:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

- After frontend tasks, run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

- For every parser/adapter task, write or extend unit tests before relying on UI smoke.
- P9 is not complete until final verification passes on `master` or the active implementation branch and the app can be manually smoked through the relevant pages.

---

## Product Boundary

P9 builds:

- durable group and rate facts;
- collector run records and structured task status;
- adapter dispatch by station type;
- Sub2API usage/session/group/rate/key-group collection;
- NewAPI access-token/user-id balance and group/rate collection;
- OpenAI-compatible model collection;
- group binding and rate history UI;
- price/rate normalization metadata;
- route economics and request-log explanations;
- change center coverage for group, rate, price, balance, collector, model, key, and route-impact events;
- settings for collector intervals and depleted-balance fallback.

P9 does not build:

- cloud sync;
- team permissions;
- SaaS accounts;
- CAPTCHA/Turnstile/2FA bypass;
- magic per-site hacks in UI or routing;
- precise cheap-first ranking from `group_rate_only` facts;
- full NewAPI username/password login.

---

## File Structure Map

### Backend Models

- Create: `src-tauri/src/models/group_facts.rs`
  - `StationGroupBinding`, `GroupRateRecord`, `UpsertStationGroupBindingInput`, `InsertGroupRateRecordInput`, status constants.
- Create: `src-tauri/src/models/collector_runs.rs`
  - `CollectorRun`, `CollectorTaskType`, `CollectorRunStatus`, `StartCollectorRunInput`, `FinishCollectorRunInput`.
- Modify: `src-tauri/src/models/collector.rs`
  - Add task-oriented fact result structs only if they are returned by commands.
- Modify: `src-tauri/src/models/pricing.rs`
  - Extend `PricingRule` and `UpsertPricingRuleInput` with station key, group binding, rate multiplier, base source, normalization status, validity range.
- Modify: `src-tauri/src/models/station_keys.rs`
  - Extend `StationKey`, `KeyPoolItem`, create/update inputs with group binding, group hash, rate multiplier, rate source, rate collected timestamp, balance scope.
- Modify: `src-tauri/src/models/settings.rs`
  - Add P9 settings.
- Modify: `src-tauri/src/models/change_events.rs`
  - Add object/event types if strongly typed later.
- Modify: `src-tauri/src/models/mod.rs`
  - Export new model modules.

### Backend Collector

- Create: `src-tauri/src/services/collectors/facts.rs`
  - Unified fact structs from adapters.
- Create: `src-tauri/src/services/collectors/url.rs`
  - URL normalization helper with `upstream_api_base_url` and `management_base_url`.
- Create: `src-tauri/src/services/collectors/session.rs`
  - Session resolution, token secret refs, refresh/login/manual token flow.
- Create: `src-tauri/src/services/collectors/apply.rs`
  - Apply facts to database and emit change events.
- Create: `src-tauri/src/services/collectors/adapters/mod.rs`
  - Adapter trait and dispatch exports.
- Create: `src-tauri/src/services/collectors/adapters/sub2api.rs`
  - Sub2API usage/session/groups/rates/key-group collection.
- Create: `src-tauri/src/services/collectors/adapters/newapi.rs`
  - NewAPI user self and groups collection.
- Create: `src-tauri/src/services/collectors/adapters/openai_compatible.rs`
  - `/v1/models` and capability/model collection.
- Modify: `src-tauri/src/services/collectors/mod.rs`
  - Dispatch by station type; expose `collect_station_task` and existing command-compatible wrappers.
- Modify: `src-tauri/src/services/collectors/sub2api.rs`
  - Keep legacy helpers temporarily only if still used by tests; move durable logic into adapters.

### Backend Persistence and Events

- Modify: `src-tauri/src/services/database.rs`
  - Schema migrations.
  - Row mappers.
  - CRUD/list helpers for group bindings, rate records, collector runs.
  - Extended pricing, key pool, settings, route economics, request logs.
  - Migration from legacy group names and snapshots.
  - Safety scan target expansion.
- Modify: `src-tauri/src/services/change_events.rs`
  - Builders for group added/missing, key binding, rate change, price expiry, route impact, collector recovery.
- Modify: `src-tauri/src/services/pricing/mod.rs`
  - Sanitizers and normalization helpers for complete/manual/group-rate-only pricing.
- Modify: `src-tauri/src/services/proxy/router.rs`
  - Extended route economics scoring and explanations.
- Modify: `src-tauri/src/services/proxy/runtime.rs`
  - Persist route economics context in request logs.
- Modify: `src-tauri/src/commands/mod.rs`
  - Commands for collector tasks, group binding, rate records, collector runs, settings.
- Modify: `src-tauri/src/lib.rs`
  - Register commands.

### Frontend Types and APIs

- Create: `src/lib/types/groupFacts.ts`
- Create: `src/lib/types/collectorRuns.ts`
- Modify: `src/lib/types/collector.ts`
- Modify: `src/lib/types/economics.ts`
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/lib/types/settings.ts`
- Modify: `src/lib/types/routing.ts`
- Modify: `src/lib/types/proxy.ts`
- Create: `src/lib/api/groupFacts.ts`
- Create: `src/lib/api/collectorRuns.ts`
- Modify: `src/lib/api/collector.ts`
- Modify: `src/lib/api/economics.ts`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src/lib/api/settings.ts`
- Modify: `src/lib/mock/*` as needed for browser fallback.

### Frontend UI

- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/stations/stationAssetViewModels.ts`
- Modify: `src/features/stations/components/StationDetailPanel.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/pricing/PricingPage.tsx`
- Modify: `src/features/pricing/pricingMatrix.ts`
- Modify: `src/features/pricing/rateSnapshotParser.ts`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/changes/ChangeCenterPage.tsx`
- Modify: `src/features/changes/changeEventViewModels.ts`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Add small UI primitives only if needed, under `src/components/ui`.

### Docs

- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/PRODUCT_MODEL.md`
- Modify: `docs/research/PRICE_MONITORING_REFERENCE_GUIDE.md` only if implementation discovers a verified correction.
- Modify: this plan if tasks change materially.

---

## Completion Gates

P9 passes only when all gates below pass.

### Functional Gates

- [ ] Existing databases migrate without losing stations, keys, pricing rules, balances, logs, or snapshots.
- [ ] `station_group_bindings` distinguishes station-level group facts from key-specific bindings.
- [ ] `group_key_hash` is non-empty for every group binding row.
- [ ] Group rows with no external group id still dedupe correctly.
- [ ] `full` collector runs create a parent run and child runs for balance/groups/models.
- [ ] Sub2API balance collection uses routeable Station Keys and correct URL normalization.
- [ ] Sub2API group/rate collection resolves session through saved access token, refresh token, password login, or manual session.
- [ ] NewAPI balance and groups work through access token + user id.
- [ ] OpenAI-compatible stations collect model list without pretending to support balance/rates.
- [ ] Key Pool shows group binding, binding status, effective multiplier, balance scope, routeability, and price state.
- [ ] Price/rate page uses matrix views fed by durable facts.
- [ ] Route explain includes group binding, rate, pricing status, balance status, freshness, and rejected candidates.
- [ ] Request logs record economic context without secrets or prompt/response content.
- [ ] Change center shows group, rate, price, balance, collector, model, key, and route-impact events.
- [ ] Collector center supports detect/balance/groups/models/full tasks and displays runs, session status, facts, endpoint results, and redacted raw.

### Security Gates

- [ ] New access token, refresh token, cookie, and password data goes through SecretManager.
- [ ] `collector_runs.error_message` stores redacted/truncated text only.
- [ ] `collector_snapshots.raw_json_redacted` remains redacted.
- [ ] New tables are included in secret safety scan.
- [ ] UI never receives full token/password/cookie values in list APIs.
- [ ] Manual token save does not echo the token back to the frontend.

### Verification Gates

- [ ] `pnpm.cmd tsc --noEmit`
- [ ] `pnpm.cmd build`
- [ ] `cargo check --manifest-path .\src-tauri\Cargo.toml`
- [ ] `cargo test --manifest-path .\src-tauri\Cargo.toml --lib`
- [ ] Manual smoke on 总览, 中转站资产, Key 池, 价格 / 倍率, 路由规则, 变更中心, 采集中心, 请求日志, 设置.

---

## Task 1: Baseline Audit and Scope Lock

**Files:**

- Read-only: repository status, P9 spec, current models/routes/database.

- [ ] **Step 1: Inspect worktree and branch**

Run:

```powershell
git status --short -- .
git diff --cached --name-only
git branch --show-current
git log --oneline -8
```

Expected:

- Record current branch.
- Record any dirty files.
- Do not edit or stage anything in this task.

- [ ] **Step 2: Read P9 spec**

Run:

```powershell
Get-Content -Raw -Encoding utf8 "docs\superpowers\specs\2026-07-03-p9-real-station-collection-routing-facts-design.md"
```

Expected:

- Confirm the spec includes `group_key_hash`, `parent_run_id`, URL helper, NewAPI adapter, and `allow_depleted_fallback`.

- [ ] **Step 3: Inspect current backend boundaries**

Run:

```powershell
Get-Content -Raw -Encoding utf8 "src-tauri\src\models\pricing.rs"
Get-Content -Raw -Encoding utf8 "src-tauri\src\models\station_keys.rs"
Get-Content -Raw -Encoding utf8 "src-tauri\src\models\settings.rs"
Get-Content -Raw -Encoding utf8 "src-tauri\src\models\collector.rs"
Get-Content -Raw -Encoding utf8 "src-tauri\src\services\collectors\mod.rs"
```

Expected:

- Note existing field names before extending them.
- Existing `collect_station_info` dispatch is still not adapter-based.

- [ ] **Step 4: Inspect current database shape**

Run:

```powershell
rg -n "CREATE TABLE IF NOT EXISTS|migrate_|pricing_rules|balance_snapshots|collector_snapshots|station_credentials|station_keys|change_events|settings" src-tauri\src\services\database.rs
```

Expected:

- Locate schema initializer and existing migration helpers.
- Locate existing safety scan targets.
- Locate pricing/balance/change-event helpers.

- [ ] **Step 5: Inspect current frontend surfaces**

Run:

```powershell
Get-Content -Raw -Encoding utf8 "src\features\stations\StationsPage.tsx"
Get-Content -Raw -Encoding utf8 "src\features\key-pool\KeyPoolPage.tsx"
Get-Content -Raw -Encoding utf8 "src\features\pricing\PricingPage.tsx"
Get-Content -Raw -Encoding utf8 "src\features\collectors\CollectorsPage.tsx"
```

Expected:

- Confirm current pages still use snapshot-derived group/rate display.
- No edits in this task.

- [ ] **Step 6: Commit**

No commit for this task. It is read-only.

---

## Task 2: Add P9 Rust Models and Type Tests

**Files:**

- Create: `src-tauri/src/models/group_facts.rs`
- Create: `src-tauri/src/models/collector_runs.rs`
- Modify: `src-tauri/src/models/pricing.rs`
- Modify: `src-tauri/src/models/station_keys.rs`
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/models/mod.rs`

- [ ] **Step 1: Create group fact model tests**

Create `src-tauri/src/models/group_facts.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const BINDING_KIND_STATION_GROUP: &str = "station_group";
pub const BINDING_KIND_KEY_BINDING: &str = "key_binding";

pub const BINDING_STATUS_AVAILABLE: &str = "available";
pub const BINDING_STATUS_BOUND: &str = "bound";
pub const BINDING_STATUS_MISSING: &str = "missing";
pub const BINDING_STATUS_DISABLED: &str = "disabled";
pub const BINDING_STATUS_MANUAL_LEGACY: &str = "manual_legacy";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationGroupBinding {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub binding_kind: String,
    pub parent_group_binding_id: Option<String>,
    pub group_key_hash: String,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub binding_status: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub rate_source: Option<String>,
    pub confidence: f64,
    pub last_seen_at: Option<String>,
    pub last_checked_at: Option<String>,
    pub last_rate_changed_at: Option<String>,
    pub raw_json_redacted: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRateRecord {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub group_binding_id: Option<String>,
    pub binding_kind: String,
    pub group_key_hash: String,
    pub group_name: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub source: String,
    pub confidence: f64,
    pub raw_json_redacted: Option<serde_json::Value>,
    pub checked_at: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertStationGroupBindingInput {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub binding_kind: String,
    pub parent_group_binding_id: Option<String>,
    pub group_key_hash: String,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub binding_status: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub rate_source: Option<String>,
    pub confidence: f64,
    pub last_seen_at: Option<String>,
    pub raw_json_redacted: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStationKeyGroupBindingInput {
    pub station_key_id: String,
    pub group_binding_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_group_binding_serializes_camel_case() {
        let binding = StationGroupBinding {
            id: "gb-1".to_string(),
            station_id: "station-1".to_string(),
            station_key_id: None,
            binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
            parent_group_binding_id: None,
            group_key_hash: "hash-1".to_string(),
            group_id_hash: Some("gid-hash".to_string()),
            group_name: "default".to_string(),
            binding_status: BINDING_STATUS_AVAILABLE.to_string(),
            default_rate_multiplier: Some(1.0),
            user_rate_multiplier: Some(0.8),
            effective_rate_multiplier: Some(0.8),
            rate_source: Some("groups_api".to_string()),
            confidence: 0.95,
            last_seen_at: Some("1000".to_string()),
            last_checked_at: Some("1000".to_string()),
            last_rate_changed_at: None,
            raw_json_redacted: None,
            created_at: "1000".to_string(),
            updated_at: "1000".to_string(),
        };
        let value = serde_json::to_value(binding).expect("json");

        assert_eq!(value["stationId"], "station-1");
        assert_eq!(value["bindingKind"], "station_group");
        assert_eq!(value["groupKeyHash"], "hash-1");
        assert_eq!(value["effectiveRateMultiplier"], 0.8);
    }
}
```

- [ ] **Step 2: Create collector run model tests**

Create `src-tauri/src/models/collector_runs.rs`:

```rust
use serde::{Deserialize, Serialize};

pub const COLLECTOR_TASK_DETECT: &str = "detect";
pub const COLLECTOR_TASK_BALANCE: &str = "balance";
pub const COLLECTOR_TASK_GROUPS: &str = "groups";
pub const COLLECTOR_TASK_MODELS: &str = "models";
pub const COLLECTOR_TASK_FULL: &str = "full";

pub const COLLECTOR_RUN_SUCCESS: &str = "success";
pub const COLLECTOR_RUN_PARTIAL: &str = "partial";
pub const COLLECTOR_RUN_FAILED: &str = "failed";
pub const COLLECTOR_RUN_MANUAL_REQUIRED: &str = "manual_required";
pub const COLLECTOR_RUN_RUNNING: &str = "running";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectorRun {
    pub id: String,
    pub station_id: String,
    pub parent_run_id: Option<String>,
    pub adapter: String,
    pub task_type: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub endpoint_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub manual_action_required: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub snapshot_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateCollectorRunInput {
    pub station_id: String,
    pub parent_run_id: Option<String>,
    pub adapter: String,
    pub task_type: String,
}

#[derive(Debug, Clone)]
pub struct FinishCollectorRunInput {
    pub id: String,
    pub status: String,
    pub endpoint_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub manual_action_required: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub snapshot_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_run_serializes_camel_case() {
        let run = CollectorRun {
            id: "run-1".to_string(),
            station_id: "station-1".to_string(),
            parent_run_id: Some("run-parent".to_string()),
            adapter: "sub2api".to_string(),
            task_type: COLLECTOR_TASK_FULL.to_string(),
            status: COLLECTOR_RUN_PARTIAL.to_string(),
            started_at: "1000".to_string(),
            finished_at: Some("1100".to_string()),
            duration_ms: Some(100),
            endpoint_count: 3,
            success_count: 2,
            failure_count: 1,
            manual_action_required: false,
            error_code: None,
            error_message: None,
            snapshot_id: Some("snapshot-1".to_string()),
            created_at: "1000".to_string(),
        };
        let value = serde_json::to_value(run).expect("json");

        assert_eq!(value["stationId"], "station-1");
        assert_eq!(value["parentRunId"], "run-parent");
        assert_eq!(value["durationMs"], 100);
        assert_eq!(value["manualActionRequired"], false);
    }
}
```

- [ ] **Step 3: Extend pricing model**

Modify `src-tauri/src/models/pricing.rs`:

Add fields to `PricingRule`:

```rust
pub station_key_id: Option<String>,
pub group_binding_id: Option<String>,
pub rate_multiplier: Option<f64>,
pub base_price_source: Option<String>,
pub normalization_status: String,
pub valid_from: Option<String>,
pub valid_until: Option<String>,
```

Add fields to `UpsertPricingRuleInput`:

```rust
pub station_key_id: Option<String>,
pub group_binding_id: Option<String>,
pub rate_multiplier: Option<f64>,
pub base_price_source: Option<String>,
pub normalization_status: Option<String>,
pub valid_from: Option<String>,
pub valid_until: Option<String>,
```

Update existing serialization test so it asserts:

```rust
assert_eq!(json["groupBindingId"], "binding-1");
assert_eq!(json["normalizationStatus"], "complete");
```

- [ ] **Step 4: Extend station key model**

Modify `src-tauri/src/models/station_keys.rs`:

Add fields to both `StationKey` and `KeyPoolItem`:

```rust
pub group_binding_id: Option<String>,
pub group_id_hash: Option<String>,
pub rate_multiplier: Option<f64>,
pub rate_source: Option<String>,
pub rate_collected_at: Option<String>,
pub balance_scope: Option<String>,
```

Add optional fields to `CreateStationKeyInput`:

```rust
pub group_binding_id: Option<String>,
pub group_id_hash: Option<String>,
pub rate_multiplier: Option<f64>,
pub rate_source: Option<String>,
pub balance_scope: Option<String>,
```

Add fields to `UpdateStationKeyInput`:

```rust
pub group_binding_id: Option<String>,
pub group_id_hash: Option<String>,
pub rate_multiplier: Option<f64>,
pub rate_source: Option<String>,
pub balance_scope: Option<String>,
```

- [ ] **Step 5: Extend settings model**

Modify `src-tauri/src/models/settings.rs`:

Add fields to `AppSettings` and `UpdateSettingsInput`:

```rust
pub balance_interval_minutes: u16,
pub group_rate_interval_minutes: u16,
pub model_list_interval_minutes: u16,
pub pricing_refresh_interval_minutes: u16,
pub collector_timeout_seconds: u16,
pub collector_max_concurrency: u16,
pub allow_depleted_fallback: bool,
```

- [ ] **Step 6: Export model modules**

Modify `src-tauri/src/models/mod.rs`:

```rust
pub mod collector_runs;
pub mod group_facts;
```

- [ ] **Step 7: Run model tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml station_group_binding_serializes_camel_case --lib
cargo test --manifest-path .\src-tauri\Cargo.toml collector_run_serializes_camel_case --lib
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_rule_serializes_camel_case --lib
```

Expected:

- All tests pass after model updates.

- [ ] **Step 8: Run Rust check**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- Check passes.
- Existing dead-code warnings may remain, but no new compile errors.

- [ ] **Step 9: Commit**

Run:

```powershell
git add -- src-tauri/src/models/group_facts.rs src-tauri/src/models/collector_runs.rs src-tauri/src/models/pricing.rs src-tauri/src/models/station_keys.rs src-tauri/src/models/settings.rs src-tauri/src/models/mod.rs
git -c gc.auto=0 commit -m "feat: add P9 collection fact models"
```

---

## Task 3: Add SQLite Schema, Migrations, and Row Mappers

**Files:**

- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add schema migration test first**

In `src-tauri/src/services/database.rs` test module, add:

```rust
#[test]
fn p9_fact_tables_are_initialized() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let connection = database.connection().expect("connection");

    for table in ["station_group_bindings", "group_rate_records", "collector_runs"] {
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                rusqlite::params![table],
                |row| row.get(0),
            )
            .expect("table count");
        assert_eq!(count, 1, "{table} should exist");
    }
}
```

- [ ] **Step 2: Run failing test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml p9_fact_tables_are_initialized --lib
```

Expected:

- FAIL because the tables do not exist yet.

- [ ] **Step 3: Add schema SQL**

In `initialize_schema` and the migration helper used by existing app databases, add:

```rust
connection.execute_batch(
    r#"
    CREATE TABLE IF NOT EXISTS station_group_bindings (
        id TEXT PRIMARY KEY,
        station_id TEXT NOT NULL,
        station_key_id TEXT,
        binding_kind TEXT NOT NULL,
        parent_group_binding_id TEXT,
        group_key_hash TEXT NOT NULL,
        group_id_hash TEXT,
        group_id_enc TEXT,
        group_name TEXT NOT NULL,
        binding_status TEXT NOT NULL,
        default_rate_multiplier REAL,
        user_rate_multiplier REAL,
        effective_rate_multiplier REAL,
        rate_source TEXT,
        confidence REAL NOT NULL DEFAULT 0.5,
        last_seen_at TEXT,
        last_checked_at TEXT,
        last_rate_changed_at TEXT,
        raw_json_redacted TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
        FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE,
        FOREIGN KEY(parent_group_binding_id) REFERENCES station_group_bindings(id) ON DELETE SET NULL
    );

    CREATE UNIQUE INDEX IF NOT EXISTS idx_group_bindings_station_group_key
        ON station_group_bindings(station_id, binding_kind, group_key_hash)
        WHERE binding_kind = 'station_group';

    CREATE UNIQUE INDEX IF NOT EXISTS idx_group_bindings_key_group_key
        ON station_group_bindings(station_key_id, binding_kind, group_key_hash)
        WHERE binding_kind = 'key_binding';

    CREATE INDEX IF NOT EXISTS idx_group_bindings_station_status
        ON station_group_bindings(station_id, binding_status, updated_at DESC);

    CREATE TABLE IF NOT EXISTS group_rate_records (
        id TEXT PRIMARY KEY,
        station_id TEXT NOT NULL,
        station_key_id TEXT,
        group_binding_id TEXT,
        binding_kind TEXT NOT NULL,
        group_key_hash TEXT NOT NULL,
        group_name TEXT NOT NULL,
        default_rate_multiplier REAL,
        user_rate_multiplier REAL,
        effective_rate_multiplier REAL,
        source TEXT NOT NULL,
        confidence REAL NOT NULL DEFAULT 0.5,
        raw_json_redacted TEXT,
        checked_at TEXT NOT NULL,
        created_at TEXT NOT NULL,
        FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
        FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE,
        FOREIGN KEY(group_binding_id) REFERENCES station_group_bindings(id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS idx_group_rate_records_binding_checked
        ON group_rate_records(group_binding_id, checked_at DESC);

    CREATE INDEX IF NOT EXISTS idx_group_rate_records_station_checked
        ON group_rate_records(station_id, checked_at DESC);

    CREATE TABLE IF NOT EXISTS collector_runs (
        id TEXT PRIMARY KEY,
        station_id TEXT NOT NULL,
        parent_run_id TEXT,
        adapter TEXT NOT NULL,
        task_type TEXT NOT NULL,
        status TEXT NOT NULL,
        started_at TEXT NOT NULL,
        finished_at TEXT,
        duration_ms INTEGER,
        endpoint_count INTEGER NOT NULL DEFAULT 0,
        success_count INTEGER NOT NULL DEFAULT 0,
        failure_count INTEGER NOT NULL DEFAULT 0,
        manual_action_required INTEGER NOT NULL DEFAULT 0,
        error_code TEXT,
        error_message TEXT,
        snapshot_id TEXT,
        created_at TEXT NOT NULL,
        FOREIGN KEY(station_id) REFERENCES stations(id) ON DELETE CASCADE,
        FOREIGN KEY(parent_run_id) REFERENCES collector_runs(id) ON DELETE CASCADE,
        FOREIGN KEY(snapshot_id) REFERENCES collector_snapshots(id) ON DELETE SET NULL
    );

    CREATE INDEX IF NOT EXISTS idx_collector_runs_station_created
        ON collector_runs(station_id, created_at DESC);

    CREATE INDEX IF NOT EXISTS idx_collector_runs_parent
        ON collector_runs(parent_run_id, created_at ASC);
    "#,
)
.map_err(|error| format!("初始化 P9 事实层 schema 失败: {error}"))?;
```

- [ ] **Step 4: Add missing columns idempotently**

Use existing `add_column_if_missing` helper to add columns:

```rust
add_column_if_missing(connection, "station_credentials", "access_token_secret_id", "TEXT")?;
add_column_if_missing(connection, "station_credentials", "refresh_token_secret_id", "TEXT")?;
add_column_if_missing(connection, "station_credentials", "cookie_secret_id", "TEXT")?;
add_column_if_missing(connection, "station_credentials", "newapi_user_id", "TEXT")?;
add_column_if_missing(connection, "station_credentials", "token_expires_at", "TEXT")?;
add_column_if_missing(connection, "station_credentials", "token_refreshed_at", "TEXT")?;
add_column_if_missing(connection, "station_credentials", "session_source", "TEXT NOT NULL DEFAULT 'none'")?;

add_column_if_missing(connection, "station_keys", "group_binding_id", "TEXT")?;
add_column_if_missing(connection, "station_keys", "group_id_hash", "TEXT")?;
add_column_if_missing(connection, "station_keys", "rate_multiplier", "REAL")?;
add_column_if_missing(connection, "station_keys", "rate_source", "TEXT")?;
add_column_if_missing(connection, "station_keys", "rate_collected_at", "TEXT")?;
add_column_if_missing(connection, "station_keys", "balance_scope", "TEXT")?;

add_column_if_missing(connection, "pricing_rules", "station_key_id", "TEXT")?;
add_column_if_missing(connection, "pricing_rules", "group_binding_id", "TEXT")?;
add_column_if_missing(connection, "pricing_rules", "rate_multiplier", "REAL")?;
add_column_if_missing(connection, "pricing_rules", "base_price_source", "TEXT")?;
add_column_if_missing(connection, "pricing_rules", "normalization_status", "TEXT NOT NULL DEFAULT 'manual'")?;
add_column_if_missing(connection, "pricing_rules", "valid_from", "TEXT")?;
add_column_if_missing(connection, "pricing_rules", "valid_until", "TEXT")?;
```

- [ ] **Step 5: Seed P9 settings**

Extend default settings seed with:

```rust
("balance_interval_minutes", "5"),
("group_rate_interval_minutes", "20"),
("model_list_interval_minutes", "60"),
("pricing_refresh_interval_minutes", "60"),
("collector_timeout_seconds", "15"),
("collector_max_concurrency", "3"),
("allow_depleted_fallback", "false"),
```

- [ ] **Step 6: Add row mappers**

Add mappers that parse JSON fields through existing parse helpers:

```rust
fn row_to_station_group_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<StationGroupBinding> {
    let raw_json: Option<String> = row.get("raw_json_redacted")?;
    Ok(StationGroupBinding {
        id: row.get("id")?,
        station_id: row.get("station_id")?,
        station_key_id: row.get("station_key_id")?,
        binding_kind: row.get("binding_kind")?,
        parent_group_binding_id: row.get("parent_group_binding_id")?,
        group_key_hash: row.get("group_key_hash")?,
        group_id_hash: row.get("group_id_hash")?,
        group_name: row.get("group_name")?,
        binding_status: row.get("binding_status")?,
        default_rate_multiplier: row.get("default_rate_multiplier")?,
        user_rate_multiplier: row.get("user_rate_multiplier")?,
        effective_rate_multiplier: row.get("effective_rate_multiplier")?,
        rate_source: row.get("rate_source")?,
        confidence: row.get("confidence")?,
        last_seen_at: row.get("last_seen_at")?,
        last_checked_at: row.get("last_checked_at")?,
        last_rate_changed_at: row.get("last_rate_changed_at")?,
        raw_json_redacted: raw_json.as_deref().map(parse_json_value),
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}
```

Also add `row_to_group_rate_record` and `row_to_collector_run` using the fields from Task 2.

- [ ] **Step 7: Run table test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml p9_fact_tables_are_initialized --lib
```

Expected:

- PASS.

- [ ] **Step 8: Run Rust check**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 9: Commit**

Run:

```powershell
git add -- src-tauri/src/services/database.rs
git -c gc.auto=0 commit -m "feat: add P9 fact schema migrations"
```

---

## Task 4: Add Database Operations for Group Bindings, Rate Records, and Collector Runs

**Files:**

- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write group key hash test**

In database tests add:

```rust
#[test]
fn group_binding_upsert_dedupes_without_external_group_id() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = create_test_station(&database, "Group Relay");

    let first = database
        .upsert_station_group_binding(UpsertStationGroupBindingInput {
            station_id: station.id.clone(),
            station_key_id: None,
            binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
            parent_group_binding_id: None,
            group_key_hash: "station:default".to_string(),
            group_id_hash: None,
            group_name: "default".to_string(),
            binding_status: BINDING_STATUS_AVAILABLE.to_string(),
            default_rate_multiplier: Some(1.0),
            user_rate_multiplier: None,
            effective_rate_multiplier: Some(1.0),
            rate_source: Some("groups_api".to_string()),
            confidence: 0.8,
            last_seen_at: None,
            raw_json_redacted: None,
        })
        .expect("first");

    let second = database
        .upsert_station_group_binding(UpsertStationGroupBindingInput {
            station_id: station.id.clone(),
            station_key_id: None,
            binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
            parent_group_binding_id: None,
            group_key_hash: "station:default".to_string(),
            group_id_hash: None,
            group_name: "default".to_string(),
            binding_status: BINDING_STATUS_AVAILABLE.to_string(),
            default_rate_multiplier: Some(1.2),
            user_rate_multiplier: None,
            effective_rate_multiplier: Some(1.2),
            rate_source: Some("groups_api".to_string()),
            confidence: 0.9,
            last_seen_at: None,
            raw_json_redacted: None,
        })
        .expect("second");

    assert_eq!(first.id, second.id);
    assert_eq!(second.effective_rate_multiplier, Some(1.2));
}
```

If no `create_test_station` helper exists, add a private test helper that calls `create_station`.

- [ ] **Step 2: Add public database methods**

Inside `impl AppDatabase` add:

```rust
pub fn list_station_group_bindings(&self, station_id: String) -> Result<Vec<StationGroupBinding>, String> {
    let connection = self.connection()?;
    list_station_group_bindings_from_connection(&connection, &station_id)
}

pub fn upsert_station_group_binding(
    &self,
    input: UpsertStationGroupBindingInput,
) -> Result<StationGroupBinding, String> {
    let connection = self.connection()?;
    upsert_station_group_binding_in_connection(&connection, input)
}

pub fn list_group_rate_records(&self, station_id: String) -> Result<Vec<GroupRateRecord>, String> {
    let connection = self.connection()?;
    list_group_rate_records_from_connection(&connection, &station_id)
}

pub fn list_collector_runs(&self, station_id: String) -> Result<Vec<CollectorRun>, String> {
    let connection = self.connection()?;
    list_collector_runs_from_connection(&connection, &station_id)
}
```

- [ ] **Step 3: Add validation helpers**

Add:

```rust
fn validate_binding_kind(value: &str) -> Result<String, String> {
    match value.trim() {
        "station_group" | "key_binding" => Ok(value.trim().to_string()),
        _ => Err("分组绑定类型无效".to_string()),
    }
}

fn validate_binding_status(value: &str) -> Result<String, String> {
    match value.trim() {
        "available" | "bound" | "missing" | "disabled" | "manual_legacy" => Ok(value.trim().to_string()),
        _ => Err("分组绑定状态无效".to_string()),
    }
}

fn validate_non_empty_hash(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err("group_key_hash 不能为空".to_string())
    } else {
        Ok(trimmed.to_string())
    }
}
```

- [ ] **Step 4: Add upsert helper**

Add helper using `ON CONFLICT` against the partial indexes by target columns:

```rust
fn upsert_station_group_binding_in_connection(
    connection: &Connection,
    input: UpsertStationGroupBindingInput,
) -> Result<StationGroupBinding, String> {
    validate_station_exists(connection, &input.station_id)?;
    if let Some(station_key_id) = input.station_key_id.as_deref() {
        validate_station_key_exists(connection, station_key_id)?;
    }

    let binding_kind = validate_binding_kind(&input.binding_kind)?;
    let binding_status = validate_binding_status(&input.binding_status)?;
    let group_key_hash = validate_non_empty_hash(&input.group_key_hash)?;
    let now = now_string();
    let raw_json = input
        .raw_json_redacted
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| format!("序列化 group raw 失败: {error}"))?;

    let existing_id = existing_group_binding_id(connection, &input.station_id, input.station_key_id.as_deref(), &binding_kind, &group_key_hash)?;
    let id = existing_id.unwrap_or_else(|| generate_id("group_binding"));

    connection
        .execute(
            "INSERT INTO station_group_bindings (
                id, station_id, station_key_id, binding_kind, parent_group_binding_id,
                group_key_hash, group_id_hash, group_name, binding_status,
                default_rate_multiplier, user_rate_multiplier, effective_rate_multiplier,
                rate_source, confidence, last_seen_at, last_checked_at, last_rate_changed_at,
                raw_json_redacted, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT(id) DO UPDATE SET
                station_key_id = excluded.station_key_id,
                parent_group_binding_id = excluded.parent_group_binding_id,
                group_id_hash = excluded.group_id_hash,
                group_name = excluded.group_name,
                binding_status = CASE
                    WHEN station_group_bindings.binding_status = 'bound' AND excluded.binding_status != 'missing'
                    THEN station_group_bindings.binding_status
                    ELSE excluded.binding_status
                END,
                default_rate_multiplier = excluded.default_rate_multiplier,
                user_rate_multiplier = excluded.user_rate_multiplier,
                effective_rate_multiplier = excluded.effective_rate_multiplier,
                rate_source = excluded.rate_source,
                confidence = excluded.confidence,
                last_seen_at = excluded.last_seen_at,
                last_checked_at = excluded.last_checked_at,
                last_rate_changed_at = excluded.last_rate_changed_at,
                raw_json_redacted = excluded.raw_json_redacted,
                updated_at = excluded.updated_at",
            rusqlite::params![
                id,
                input.station_id,
                input.station_key_id,
                binding_kind,
                input.parent_group_binding_id,
                group_key_hash,
                normalize_optional_string(input.group_id_hash),
                input.group_name.trim(),
                binding_status,
                input.default_rate_multiplier,
                input.user_rate_multiplier,
                input.effective_rate_multiplier,
                normalize_optional_string(input.rate_source),
                clamp_confidence(input.confidence),
                normalize_optional_string(input.last_seen_at),
                now,
                None::<String>,
                raw_json,
                now,
                now,
            ],
        )
        .map_err(|error| format!("保存分组绑定失败: {error}"))?;

    station_group_binding_by_id(connection, &id)
}
```

- [ ] **Step 5: Add list helpers**

Add:

```rust
fn list_station_group_bindings_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<StationGroupBinding>, String> {
    let mut statement = connection
        .prepare(
            "SELECT * FROM station_group_bindings
             WHERE station_id = ?1
             ORDER BY binding_kind ASC, binding_status ASC, group_name ASC",
        )
        .map_err(|error| format!("读取分组绑定失败: {error}"))?;
    statement
        .query_map(rusqlite::params![station_id], row_to_station_group_binding)
        .map_err(|error| format!("查询分组绑定失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析分组绑定失败: {error}"))
}
```

Add equivalent `list_group_rate_records_from_connection` and `list_collector_runs_from_connection`.

- [ ] **Step 6: Add commands**

In `src-tauri/src/commands/mod.rs` add:

```rust
#[tauri::command]
pub fn list_station_group_bindings(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<StationGroupBinding>, String> {
    database.list_station_group_bindings(station_id)
}

#[tauri::command]
pub fn upsert_station_group_binding(
    database: State<'_, AppDatabase>,
    input: UpsertStationGroupBindingInput,
) -> Result<StationGroupBinding, String> {
    database.upsert_station_group_binding(input)
}

#[tauri::command]
pub fn list_group_rate_records(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<GroupRateRecord>, String> {
    database.list_group_rate_records(station_id)
}

#[tauri::command]
pub fn list_collector_runs(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<CollectorRun>, String> {
    database.list_collector_runs(station_id)
}
```

Register commands in `src-tauri/src/lib.rs`.

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml group_binding_upsert_dedupes_without_external_group_id --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 8: Commit**

Run:

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git -c gc.auto=0 commit -m "feat: add P9 fact database operations"
```

---

## Task 5: Implement Legacy Migration into Group Facts

**Files:**

- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write migration idempotency test**

Add database test:

```rust
#[test]
fn legacy_key_group_name_migrates_to_group_binding_once() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = create_test_station(&database, "Legacy Relay");
    let key = database
        .create_station_key(CreateStationKeyInput {
            station_id: station.id.clone(),
            name: "legacy key".to_string(),
            api_key: "sk-legacy".to_string(),
            enabled: true,
            priority: Some(0),
            group_name: Some("legacy-group".to_string()),
            tier_label: None,
            note: None,
            group_binding_id: None,
            group_id_hash: None,
            rate_multiplier: None,
            rate_source: None,
            balance_scope: None,
        })
        .expect("key");

    {
        let connection = database.connection().expect("connection");
        migrate_legacy_group_facts(&connection).expect("migrate once");
        migrate_legacy_group_facts(&connection).expect("migrate twice");
    }

    let bindings = database
        .list_station_group_bindings(station.id.clone())
        .expect("bindings");
    let key_bindings = bindings
        .iter()
        .filter(|binding| binding.station_key_id.as_deref() == Some(key.id.as_str()))
        .collect::<Vec<_>>();

    assert_eq!(key_bindings.len(), 1);
    assert_eq!(key_bindings[0].binding_status, "manual_legacy");
}
```

- [ ] **Step 2: Add stable group key hash helper**

Add:

```rust
fn stable_group_key_hash(station_id: &str, adapter: &str, group_id: Option<&str>, group_name: &str) -> String {
    let source = if let Some(group_id) = group_id.filter(|value| !value.trim().is_empty()) {
        format!("id:{}:{}", adapter.trim().to_lowercase(), group_id.trim())
    } else {
        format!(
            "name:{}:{}:{}",
            station_id,
            adapter.trim().to_lowercase(),
            group_name.trim().to_lowercase()
        )
    };
    sha256_hex(source.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
```

If `sha2` is not already in `src-tauri/Cargo.toml`, add it and commit Cargo changes with this task.

- [ ] **Step 3: Add migration function**

Add:

```rust
fn migrate_legacy_group_facts(connection: &Connection) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, group_name FROM station_keys
             WHERE group_name IS NOT NULL AND TRIM(group_name) != ''",
        )
        .map_err(|error| format!("读取旧 key 分组失败: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("查询旧 key 分组失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析旧 key 分组失败: {error}"))?;

    for (station_key_id, station_id, group_name) in rows {
        let group_key_hash = stable_group_key_hash(&station_id, "legacy", None, &group_name);
        let station_binding = upsert_station_group_binding_in_connection(
            connection,
            UpsertStationGroupBindingInput {
                station_id: station_id.clone(),
                station_key_id: None,
                binding_kind: "station_group".to_string(),
                parent_group_binding_id: None,
                group_key_hash: group_key_hash.clone(),
                group_id_hash: None,
                group_name: group_name.clone(),
                binding_status: "manual_legacy".to_string(),
                default_rate_multiplier: None,
                user_rate_multiplier: None,
                effective_rate_multiplier: None,
                rate_source: Some("legacy_key_group".to_string()),
                confidence: 0.4,
                last_seen_at: None,
                raw_json_redacted: None,
            },
        )?;
        let key_binding = upsert_station_group_binding_in_connection(
            connection,
            UpsertStationGroupBindingInput {
                station_id,
                station_key_id: Some(station_key_id.clone()),
                binding_kind: "key_binding".to_string(),
                parent_group_binding_id: Some(station_binding.id),
                group_key_hash,
                group_id_hash: None,
                group_name,
                binding_status: "manual_legacy".to_string(),
                default_rate_multiplier: None,
                user_rate_multiplier: None,
                effective_rate_multiplier: None,
                rate_source: Some("legacy_key_group".to_string()),
                confidence: 0.4,
                last_seen_at: None,
                raw_json_redacted: None,
            },
        )?;
        connection
            .execute(
                "UPDATE station_keys
                 SET group_binding_id = ?1,
                     group_id_hash = ?2,
                     updated_at = ?3
                 WHERE id = ?4",
                rusqlite::params![key_binding.id, key_binding.group_key_hash, now_string(), station_key_id],
            )
            .map_err(|error| format!("回填 key 分组绑定失败: {error}"))?;
    }
    Ok(())
}
```

- [ ] **Step 4: Call migration during initialization**

In `initialize` and `new_in_memory_for_tests`, after schema migrations and default key migration, call:

```rust
migrate_legacy_group_facts(&connection)
    .map_err(|error| format!("迁移旧分组事实失败: {error}"))?;
```

- [ ] **Step 5: Expand safety scan targets**

Add these targets to secret safety scan:

```rust
("station_group_bindings", "raw_json_redacted"),
("group_rate_records", "raw_json_redacted"),
("collector_runs", "error_message"),
```

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml legacy_key_group_name_migrates_to_group_binding_once --lib
cargo test --manifest-path .\src-tauri\Cargo.toml p9_fact_tables_are_initialized --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 7: Commit**

Run:

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git -c gc.auto=0 commit -m "feat: migrate legacy group facts"
```

If `sha2` was already available and Cargo files did not change, do not stage Cargo files.

---

## Task 6: Add Collector Facts, URL Helper, and Adapter Trait

**Files:**

- Create: `src-tauri/src/services/collectors/facts.rs`
- Create: `src-tauri/src/services/collectors/url.rs`
- Create: `src-tauri/src/services/collectors/adapters/mod.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`

- [ ] **Step 1: Add URL helper tests**

Create `src-tauri/src/services/collectors/url.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectorBaseUrls {
    pub upstream_api_base_url: String,
    pub management_base_url: String,
}

pub fn collector_base_urls(base_url: &str) -> CollectorBaseUrls {
    let trimmed = base_url.trim().trim_end_matches('/').to_string();
    let management = trimmed
        .strip_suffix("/v1")
        .or_else(|| trimmed.strip_suffix("/compatible-mode/v1"))
        .unwrap_or(trimmed.as_str())
        .trim_end_matches('/')
        .to_string();
    let upstream = if trimmed.ends_with("/v1") {
        trimmed
    } else {
        format!("{management}/v1")
    };
    CollectorBaseUrls {
        upstream_api_base_url: upstream,
        management_base_url: management,
    }
}

pub fn join_url(base: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_base_urls_strip_v1_for_management() {
        let urls = collector_base_urls("https://relay.example.com/v1");
        assert_eq!(urls.upstream_api_base_url, "https://relay.example.com/v1");
        assert_eq!(urls.management_base_url, "https://relay.example.com");
        assert_eq!(
            join_url(&urls.management_base_url, "/api/v1/groups/available"),
            "https://relay.example.com/api/v1/groups/available"
        );
    }

    #[test]
    fn collector_base_urls_add_v1_for_root_input() {
        let urls = collector_base_urls("https://relay.example.com");
        assert_eq!(urls.upstream_api_base_url, "https://relay.example.com/v1");
        assert_eq!(urls.management_base_url, "https://relay.example.com");
    }
}
```

- [ ] **Step 2: Create facts module**

Create `src-tauri/src/services/collectors/facts.rs`:

```rust
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct CollectedBalanceFact {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub scope: String,
    pub value: Option<f64>,
    pub used_value: Option<f64>,
    pub total_value: Option<f64>,
    pub currency: String,
    pub credit_unit: Option<String>,
    pub status: String,
    pub source: String,
    pub confidence: f64,
    pub collected_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CollectedGroupFact {
    pub station_id: String,
    pub group_id: Option<String>,
    pub group_key_hash: String,
    pub group_name: String,
    pub visibility: String,
    pub source: String,
    pub confidence: f64,
    pub raw_json_redacted: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct CollectedRateFact {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub group_id: Option<String>,
    pub group_key_hash: String,
    pub group_name: String,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub source: String,
    pub confidence: f64,
    pub checked_at: Option<String>,
    pub raw_json_redacted: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct CollectedModelFact {
    pub station_id: String,
    pub model: String,
    pub available: bool,
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct CollectorDiagnosticFact {
    pub endpoint: String,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub error_code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ManualActionRequiredFact {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct CollectorFacts {
    pub balances: Vec<CollectedBalanceFact>,
    pub groups: Vec<CollectedGroupFact>,
    pub rates: Vec<CollectedRateFact>,
    pub models: Vec<CollectedModelFact>,
    pub diagnostics: Vec<CollectorDiagnosticFact>,
    pub manual_action: Option<ManualActionRequiredFact>,
}
```

- [ ] **Step 3: Create adapter trait**

Create `src-tauri/src/services/collectors/adapters/mod.rs`:

```rust
pub mod newapi;
pub mod openai_compatible;
pub mod sub2api;

use crate::services::collectors::facts::CollectorFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectorTask {
    Detect,
    Balance,
    Groups,
    Models,
    Full,
}

impl CollectorTask {
    pub fn as_str(self) -> &'static str {
        match self {
            CollectorTask::Detect => "detect",
            CollectorTask::Balance => "balance",
            CollectorTask::Groups => "groups",
            CollectorTask::Models => "models",
            CollectorTask::Full => "full",
        }
    }
}

pub struct AdapterOutput {
    pub adapter: String,
    pub task: CollectorTask,
    pub status: String,
    pub facts: CollectorFacts,
    pub summary_json: serde_json::Value,
    pub normalized_json: serde_json::Value,
    pub raw_json_redacted: Option<serde_json::Value>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}
```

- [ ] **Step 4: Register modules**

Modify `src-tauri/src/services/collectors/mod.rs`:

```rust
pub mod adapters;
pub mod facts;
pub mod url;
```

Keep existing public functions compiling.

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml collector_base_urls_strip_v1_for_management --lib
cargo test --manifest-path .\src-tauri\Cargo.toml collector_base_urls_add_v1_for_root_input --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 6: Commit**

Run:

```powershell
git add -- src-tauri/src/services/collectors/facts.rs src-tauri/src/services/collectors/url.rs src-tauri/src/services/collectors/adapters/mod.rs src-tauri/src/services/collectors/mod.rs
git -c gc.auto=0 commit -m "feat: add collector facts adapter framework"
```

---

## Task 7: Add Collector Run Lifecycle and Apply Layer

**Files:**

- Create: `src-tauri/src/services/collectors/apply.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add collector run lifecycle test**

In database tests add:

```rust
#[test]
fn full_collector_run_can_track_child_runs() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = create_test_station(&database, "Run Relay");

    let parent = database
        .create_collector_run(CreateCollectorRunInput {
            station_id: station.id.clone(),
            parent_run_id: None,
            adapter: "sub2api".to_string(),
            task_type: "full".to_string(),
        })
        .expect("parent");
    let child = database
        .create_collector_run(CreateCollectorRunInput {
            station_id: station.id.clone(),
            parent_run_id: Some(parent.id.clone()),
            adapter: "sub2api".to_string(),
            task_type: "balance".to_string(),
        })
        .expect("child");
    database
        .finish_collector_run(FinishCollectorRunInput {
            id: child.id.clone(),
            status: "success".to_string(),
            endpoint_count: 1,
            success_count: 1,
            failure_count: 0,
            manual_action_required: false,
            error_code: None,
            error_message: None,
            snapshot_id: None,
        })
        .expect("finish child");

    let runs = database.list_collector_runs(station.id).expect("runs");
    assert!(runs.iter().any(|run| run.parent_run_id.as_deref() == Some(parent.id.as_str())));
}
```

- [ ] **Step 2: Add database lifecycle methods**

Inside `AppDatabase` add:

```rust
pub fn create_collector_run(&self, input: CreateCollectorRunInput) -> Result<CollectorRun, String> {
    let connection = self.connection()?;
    create_collector_run_in_connection(&connection, input)
}

pub fn finish_collector_run(&self, input: FinishCollectorRunInput) -> Result<CollectorRun, String> {
    let connection = self.connection()?;
    finish_collector_run_in_connection(&connection, input)
}
```

Add helpers to insert/update `collector_runs`. Redact `error_message` with shared mask:

```rust
let error_message = input
    .error_message
    .as_deref()
    .map(crate::services::secrets::mask::redact_text);
```

- [ ] **Step 3: Create apply layer**

Create `src-tauri/src/services/collectors/apply.rs`:

```rust
use crate::{
    models::pricing::UpsertBalanceSnapshotInput,
    services::{
        collectors::facts::CollectorFacts,
        database::AppDatabase,
    },
};

pub fn apply_collector_facts(database: &AppDatabase, facts: CollectorFacts) -> Result<(), String> {
    for balance in facts.balances {
        database.upsert_balance_snapshot(UpsertBalanceSnapshotInput {
            id: None,
            station_id: balance.station_id,
            station_key_id: balance.station_key_id,
            scope: balance.scope,
            value: balance.value,
            currency: balance.currency,
            credit_unit: balance.credit_unit,
            used_value: balance.used_value,
            total_value: balance.total_value,
            low_balance_threshold: None,
            status: balance.status,
            source: balance.source,
            confidence: balance.confidence,
            collected_at: balance.collected_at,
        })?;
    }
    for group in facts.groups {
        database.upsert_station_group_binding(crate::models::group_facts::UpsertStationGroupBindingInput {
            station_id: group.station_id,
            station_key_id: None,
            binding_kind: "station_group".to_string(),
            parent_group_binding_id: None,
            group_key_hash: group.group_key_hash,
            group_id_hash: group.group_id,
            group_name: group.group_name,
            binding_status: "available".to_string(),
            default_rate_multiplier: None,
            user_rate_multiplier: None,
            effective_rate_multiplier: None,
            rate_source: Some(group.source),
            confidence: group.confidence,
            last_seen_at: None,
            raw_json_redacted: group.raw_json_redacted,
        })?;
    }
    Ok(())
}
```

Later tasks extend this function for rates, models, and pricing.

- [ ] **Step 4: Register apply module**

Modify `src-tauri/src/services/collectors/mod.rs`:

```rust
pub mod apply;
```

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml full_collector_run_can_track_child_runs --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 6: Commit**

Run:

```powershell
git add -- src-tauri/src/services/collectors/apply.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/database.rs
git -c gc.auto=0 commit -m "feat: add collector run lifecycle"
```

---

## Task 8: Implement Sub2API Usage Parser and Balance Collection

**Files:**

- Modify: `src-tauri/src/services/collectors/adapters/sub2api.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`

- [ ] **Step 1: Add parser tests**

Create or edit `src-tauri/src/services/collectors/adapters/sub2api.rs`:

```rust
use serde_json::{json, Value};

use crate::services::collectors::facts::CollectedBalanceFact;

fn parse_usage_balance(
    station_id: &str,
    station_key_id: Option<String>,
    payload: &Value,
) -> CollectedBalanceFact {
    let remaining = payload
        .pointer("/quota/remaining")
        .and_then(Value::as_f64)
        .or_else(|| payload.get("remaining").and_then(Value::as_f64))
        .or_else(|| payload.get("balance").and_then(Value::as_f64));
    let used = payload
        .pointer("/quota/used")
        .and_then(Value::as_f64)
        .or_else(|| payload.get("used").and_then(Value::as_f64));
    let total = payload
        .pointer("/quota/total")
        .and_then(Value::as_f64)
        .or_else(|| payload.get("total").and_then(Value::as_f64));
    let status = if remaining == Some(0.0) {
        "depleted"
    } else {
        "normal"
    };
    CollectedBalanceFact {
        station_id: station_id.to_string(),
        station_key_id,
        scope: "station_key".to_string(),
        value: remaining,
        used_value: used,
        total_value: total,
        currency: "CNY".to_string(),
        credit_unit: payload
            .pointer("/quota/unit")
            .and_then(Value::as_str)
            .or_else(|| payload.get("unit").and_then(Value::as_str))
            .map(ToString::to_string),
        status: status.to_string(),
        source: "sub2api_usage".to_string(),
        confidence: if remaining.is_some() { 0.9 } else { 0.4 },
        collected_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub2api_usage_parses_remaining_from_nested_quota() {
        let fact = parse_usage_balance(
            "station-1",
            Some("key-1".to_string()),
            &json!({
                "quota": {
                    "remaining": 42.5,
                    "used": 10.0,
                    "total": 52.5,
                    "unit": "CNY"
                },
                "planName": "pro"
            }),
        );

        assert_eq!(fact.station_id, "station-1");
        assert_eq!(fact.station_key_id.as_deref(), Some("key-1"));
        assert_eq!(fact.value, Some(42.5));
        assert_eq!(fact.used_value, Some(10.0));
        assert_eq!(fact.total_value, Some(52.5));
        assert_eq!(fact.source, "sub2api_usage");
    }

    #[test]
    fn sub2api_usage_marks_zero_balance_depleted() {
        let fact = parse_usage_balance("station-1", None, &json!({ "remaining": 0.0 }));
        assert_eq!(fact.status, "depleted");
    }
}
```

- [ ] **Step 2: Add routeable key selection**

Add helper:

```rust
fn routeable_keys_for_station(database: &AppDatabase, station_id: &str) -> Result<Vec<crate::models::station_keys::StationKey>, String> {
    database
        .list_station_keys(station_id.to_string())
        .map(|keys| keys.into_iter().filter(|key| key.enabled && key.api_key_present).collect())
}
```

- [ ] **Step 3: Add balance collection function**

Add:

```rust
pub fn collect_balance(database: &AppDatabase, data_key: &[u8; 32], station_id: &str) -> Result<AdapterOutput, String> {
    let station = database.station_for_collector(station_id)?;
    let keys = routeable_keys_for_station(database, station_id)?;
    let urls = crate::services::collectors::url::collector_base_urls(&station.base_url);
    let mut facts = crate::services::collectors::facts::CollectorFacts::default();
    let mut endpoint_results = Vec::new();

    for key in keys {
        let Some(api_key) = database.resolve_station_key_secret_for_tests(key.id.clone(), data_key)? else {
            continue;
        };
        let url = crate::services::collectors::url::join_url(&urls.upstream_api_base_url, "/usage");
        let started = std::time::Instant::now();
        let response = match ureq::get(&url).set("Authorization", &format!("Bearer {api_key}")).call() {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(error) => {
                endpoint_results.push(json!({
                    "endpoint": url,
                    "status": "network_error",
                    "message": crate::services::secrets::mask::redact_text(&error.to_string()),
                    "durationMs": started.elapsed().as_millis() as i64
                }));
                continue;
            }
        };
        let status = response.status();
        let text = response.into_string().unwrap_or_default();
        let parsed = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
        endpoint_results.push(json!({
            "endpoint": url,
            "status": status,
            "durationMs": started.elapsed().as_millis() as i64,
            "ok": (200..400).contains(&status)
        }));
        if (200..400).contains(&status) {
            facts.balances.push(parse_usage_balance(&station.id, Some(key.id), &parsed));
        }
    }

    Ok(AdapterOutput {
        adapter: "sub2api".to_string(),
        task: CollectorTask::Balance,
        status: if facts.balances.is_empty() { "failed" } else { "success" }.to_string(),
        summary_json: json!({ "adapter": "sub2api", "task": "balance", "endpointResults": endpoint_results }),
        normalized_json: json!({ "balances": facts.balances.len() }),
        raw_json_redacted: Some(json!({ "endpointResults": endpoint_results })),
        error_code: None,
        error_message: None,
        facts,
    })
}
```

If `resolve_station_key_secret_for_tests` is test-only in current code, add a production method that resolves the station key secret with `data_key` and call that instead.

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_usage_parses_remaining_from_nested_quota --lib
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_usage_marks_zero_balance_depleted --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 5: Commit**

Run:

```powershell
git add -- src-tauri/src/services/collectors/adapters/sub2api.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/database.rs
git -c gc.auto=0 commit -m "feat: collect Sub2API usage balance facts"
```

---

## Task 9: Implement Session Storage Fields and Session Resolver

**Files:**

- Create: `src-tauri/src/services/collectors/session.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/station_keys.rs` only if credential output types live there.

- [ ] **Step 1: Add session resolver tests**

Create `src-tauri/src/services/collectors/session.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionResolveStatus {
    Ready,
    ManualRequired,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ResolvedSession {
    pub status: SessionResolveStatus,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub message: Option<String>,
}

pub fn token_is_fresh(expires_at: Option<&str>, now_ms: i64) -> bool {
    expires_at
        .and_then(|value| value.parse::<i64>().ok())
        .map(|expires| expires > now_ms + 60_000)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_freshness_uses_sixty_second_refresh_window() {
        assert!(token_is_fresh(Some("200000"), 100000));
        assert!(!token_is_fresh(Some("150000"), 100000));
        assert!(!token_is_fresh(None, 100000));
    }
}
```

- [ ] **Step 2: Add station credential fields to database reads/writes**

Update `station_credentials_from_connection`, `upsert_station_credentials_with_data_key`, and output model to include:

```rust
access_token_present: bool,
refresh_token_present: bool,
cookie_present: bool,
newapi_user_id: Option<String>,
token_expires_at: Option<String>,
token_refreshed_at: Option<String>,
session_source: String,
```

Do not return token values.

- [ ] **Step 3: Add save manual session command**

Add an input model if needed:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStationSessionInput {
    pub station_id: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub cookie: Option<String>,
    pub newapi_user_id: Option<String>,
    pub token_expires_at: Option<String>,
}
```

Add database method that stores token/cookie through SecretManager using existing `upsert_secret_in_connection` style helpers, sets `session_source = manual_token`, and returns only presence fields.

- [ ] **Step 4: Add command**

In `src-tauri/src/commands/mod.rs`:

```rust
#[tauri::command]
pub fn update_station_session(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: UpdateStationSessionInput,
) -> Result<StationCredentials, String> {
    database.update_station_session_with_data_key(input, secrets.data_key())
}
```

Register it in `src-tauri/src/lib.rs`.

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml token_freshness_uses_sixty_second_refresh_window --lib
cargo test --manifest-path .\src-tauri\Cargo.toml encrypted_station_credentials_write_keeps_plain_password_empty --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.
- No command returns raw tokens.

- [ ] **Step 6: Commit**

Run:

```powershell
git add -- src-tauri/src/services/collectors/session.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/models/station_keys.rs
git -c gc.auto=0 commit -m "feat: add collector session credential storage"
```

Do not stage `src-tauri/src/models/station_keys.rs` if credential models live elsewhere and it did not change.

---

## Task 10: Implement Sub2API Groups, Rates, and Key Binding

**Files:**

- Modify: `src-tauri/src/services/collectors/adapters/sub2api.rs`
- Modify: `src-tauri/src/services/collectors/apply.rs`
- Modify: `src-tauri/src/services/change_events.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add group/rate join tests**

In `sub2api.rs` tests add:

```rust
#[test]
fn sub2api_groups_rates_join_by_group_id() {
    let available = serde_json::json!({
        "data": [
            { "id": "default", "name": "Default", "rate_multiplier": 1.0 },
            { "id": "pro", "name": "Pro", "rate_multiplier": 1.5 }
        ]
    });
    let rates = serde_json::json!({
        "data": {
            "default": 0.8,
            "pro": 1.2
        }
    });

    let facts = parse_group_rate_facts("station-1", &available, &rates);

    assert!(facts.groups.iter().any(|group| group.group_name == "Default"));
    assert!(facts.rates.iter().any(|rate| {
        rate.group_name == "Pro" && rate.effective_rate_multiplier == Some(1.2)
    }));
}
```

- [ ] **Step 2: Implement parser function**

Add:

```rust
fn parse_group_rate_facts(
    station_id: &str,
    available: &Value,
    rates: &Value,
) -> crate::services::collectors::facts::CollectorFacts {
    let mut facts = crate::services::collectors::facts::CollectorFacts::default();
    let groups = collect_available_groups(available);
    let rate_map = collect_user_rate_map(rates);

    for group in groups {
        let group_id = group.group_id.clone();
        let effective = group_id
            .as_deref()
            .and_then(|id| rate_map.get(id).copied())
            .or(group.default_rate_multiplier);
        facts.groups.push(CollectedGroupFact {
            station_id: station_id.to_string(),
            group_id: group_id.clone(),
            group_key_hash: stable_group_key_hash(station_id, "sub2api", group_id.as_deref(), &group.group_name),
            group_name: group.group_name.clone(),
            visibility: "available".to_string(),
            source: "sub2api_groups_available".to_string(),
            confidence: 0.9,
            raw_json_redacted: group.raw_json_redacted.clone(),
        });
        facts.rates.push(CollectedRateFact {
            station_id: station_id.to_string(),
            station_key_id: None,
            group_id,
            group_key_hash: stable_group_key_hash(station_id, "sub2api", group.group_id.as_deref(), &group.group_name),
            group_name: group.group_name,
            default_rate_multiplier: group.default_rate_multiplier,
            user_rate_multiplier: group.group_id.as_deref().and_then(|id| rate_map.get(id).copied()),
            effective_rate_multiplier: effective,
            source: "sub2api_groups_rates".to_string(),
            confidence: if effective.is_some() { 0.9 } else { 0.6 },
            checked_at: None,
            raw_json_redacted: None,
        });
    }
    facts
}
```

Add local helper structs/functions `collect_available_groups` and `collect_user_rate_map` using serde_json traversal from the existing legacy normalizer where possible.

- [ ] **Step 3: Extend apply layer for rates**

In `apply_collector_facts`, after group upsert, upsert rates and insert rate history only when changed. Add database helper `upsert_group_rate_record_if_changed`:

```rust
pub fn upsert_group_rate_record_if_changed(
    &self,
    input: InsertGroupRateRecordInput,
) -> Result<Option<GroupRateRecord>, String> {
    let connection = self.connection()?;
    insert_group_rate_record_if_changed_in_connection(&connection, input)
}
```

The helper compares previous newest record for same `station_id + binding_kind + group_key_hash` and inserts only if group name or any multiplier changed.

- [ ] **Step 4: Add rate changed event**

In `change_events.rs`, ensure `rate_changed_event` accepts station id, group name, old multiplier, new multiplier and emits:

- warning if new > old;
- info if new < old;
- dedupe key `rate_changed:station:{station_id}:group:{group_name}`.

- [ ] **Step 5: Add key group binding resolve**

Implement `resolve_key_group` using:

1. existing manual `station_keys.group_binding_id`;
2. usage plan/group name;
3. only one available station group;
4. keys endpoint if available.

When confidence is low, set `rate_source = "single_group_low_confidence"` and confidence `0.5`.

- [ ] **Step 6: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml sub2api_groups_rates_join_by_group_id --lib
cargo test --manifest-path .\src-tauri\Cargo.toml group_rate_history --lib
cargo test --manifest-path .\src-tauri\Cargo.toml rate_changed --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- The explicit parser test passes.
- Existing matching test names may differ; if they do, run the new exact names added in this task.

- [ ] **Step 7: Commit**

Run:

```powershell
git add -- src-tauri/src/services/collectors/adapters/sub2api.rs src-tauri/src/services/collectors/apply.rs src-tauri/src/services/change_events.rs src-tauri/src/services/database.rs
git -c gc.auto=0 commit -m "feat: collect Sub2API group rate facts"
```

---

## Task 11: Implement NewAPI Adapter

**Files:**

- Create or modify: `src-tauri/src/services/collectors/adapters/newapi.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/collectors/apply.rs` only if NewAPI needs a small fact mapping adjustment.

- [ ] **Step 1: Add quota conversion tests**

Create `newapi.rs`:

```rust
use serde_json::Value;

use crate::services::collectors::facts::CollectedBalanceFact;

fn parse_newapi_balance(station_id: &str, payload: &Value) -> CollectedBalanceFact {
    let quota = payload.get("quota").and_then(Value::as_f64);
    let used_quota = payload.get("used_quota").and_then(Value::as_f64);
    let remaining = quota.map(|value| value / 500000.0);
    let used = used_quota.map(|value| value / 500000.0);
    let total = match (remaining, used) {
        (Some(remaining), Some(used)) => Some(remaining + used),
        _ => None,
    };
    CollectedBalanceFact {
        station_id: station_id.to_string(),
        station_key_id: None,
        scope: "station".to_string(),
        value: remaining,
        used_value: used,
        total_value: total,
        currency: "USD".to_string(),
        credit_unit: Some("newapi_quota_500000".to_string()),
        status: if remaining == Some(0.0) { "depleted" } else { "normal" }.to_string(),
        source: "newapi_user_self".to_string(),
        confidence: if remaining.is_some() { 0.9 } else { 0.4 },
        collected_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn newapi_quota_converts_to_usd_units() {
        let fact = parse_newapi_balance(
            "station-1",
            &json!({
                "quota": 1000000.0,
                "used_quota": 500000.0,
                "group": "default"
            }),
        );

        assert_eq!(fact.value, Some(2.0));
        assert_eq!(fact.used_value, Some(1.0));
        assert_eq!(fact.total_value, Some(3.0));
        assert_eq!(fact.currency, "USD");
        assert_eq!(fact.source, "newapi_user_self");
    }
}
```

- [ ] **Step 2: Add group parser test**

Add:

```rust
#[test]
fn newapi_groups_parse_list_and_rate_fields() {
    let facts = parse_newapi_group_facts(
        "station-1",
        &json!({
            "groups": [
                { "id": "default", "name": "Default", "rate": 1.0 },
                { "id": "vip", "name": "VIP", "rateMultiplier": 0.8 }
            ]
        }),
    );

    assert!(facts.groups.iter().any(|group| group.group_name == "Default"));
    assert!(facts.rates.iter().any(|rate| {
        rate.group_name == "VIP" && rate.effective_rate_multiplier == Some(0.8)
    }));
}
```

- [ ] **Step 3: Implement group parser**

Implement `parse_newapi_group_facts` to accept:

- `user_group`;
- `userGroup`;
- `groups`;
- `items`;
- `list`;
- object map forms.

Use field priority:

```rust
fn rate_from_group_value(value: &Value) -> Option<f64> {
    value.get("effective_rate_multiplier").and_then(Value::as_f64)
        .or_else(|| value.get("rateMultiplier").and_then(Value::as_f64))
        .or_else(|| value.get("rate_multiplier").and_then(Value::as_f64))
        .or_else(|| value.get("user_rate_multiplier").and_then(Value::as_f64))
        .or_else(|| value.get("default_rate_multiplier").and_then(Value::as_f64))
        .or_else(|| value.get("ratio").and_then(Value::as_f64))
        .or_else(|| value.get("rate").and_then(Value::as_f64))
}
```

- [ ] **Step 4: Implement HTTP collection**

Add:

```rust
pub fn collect_balance_and_groups(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
    task: CollectorTask,
) -> Result<AdapterOutput, String> {
    let station = database.station_for_collector(station_id)?;
    let credentials = database.get_station_credentials(station_id.to_string())?;
    let Some(user_id) = credentials.newapi_user_id.clone() else {
        return manual_required_output("newapi", task, "newapi_user_id_required", "NewAPI 采集需要 User ID。");
    };
    let Some(access_token) = database.resolve_station_access_token_with_data_key(station_id.to_string(), data_key)? else {
        return manual_required_output("newapi", task, "manual_session_required", "NewAPI 采集需要 access token。");
    };
    let urls = crate::services::collectors::url::collector_base_urls(&station.base_url);
    let mut facts = CollectorFacts::default();
    let mut endpoint_results = Vec::new();

    if matches!(task, CollectorTask::Balance | CollectorTask::Full) {
        let url = crate::services::collectors::url::join_url(&urls.management_base_url, "/api/user/self");
        let payload = get_newapi_json(&url, &access_token, &user_id, &mut endpoint_results)?;
        facts.balances.push(parse_newapi_balance(station_id, &payload));
    }
    if matches!(task, CollectorTask::Groups | CollectorTask::Full) {
        let url = crate::services::collectors::url::join_url(&urls.management_base_url, "/api/user/self/groups");
        let payload = get_newapi_json(&url, &access_token, &user_id, &mut endpoint_results)?;
        let group_facts = parse_newapi_group_facts(station_id, &payload);
        facts.groups.extend(group_facts.groups);
        facts.rates.extend(group_facts.rates);
    }
    Ok(AdapterOutput {
        adapter: "newapi".to_string(),
        task,
        status: "success".to_string(),
        summary_json: serde_json::json!({ "adapter": "newapi", "task": task.as_str(), "endpointResults": endpoint_results }),
        normalized_json: serde_json::json!({
            "balanceCount": facts.balances.len(),
            "groupCount": facts.groups.len(),
            "rateCount": facts.rates.len()
        }),
        raw_json_redacted: Some(serde_json::json!({ "endpointResults": endpoint_results })),
        error_code: None,
        error_message: None,
        facts,
    })
}
```

Add `get_newapi_json` that sends:

```rust
.set("Authorization", &format!("Bearer {access_token}"))
.set("New-Api-User", user_id)
.set("Content-Type", "application/json")
```

Ensure all errors pass through `redact_text`.

- [ ] **Step 5: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml newapi_quota_converts_to_usd_units --lib
cargo test --manifest-path .\src-tauri\Cargo.toml newapi_groups_parse_list_and_rate_fields --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 6: Commit**

Run:

```powershell
git add -- src-tauri/src/services/collectors/adapters/newapi.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/collectors/apply.rs src-tauri/src/services/database.rs
git -c gc.auto=0 commit -m "feat: add NewAPI collector adapter"
```

Only stage `apply.rs` and `database.rs` if changed.

---

## Task 12: Implement OpenAI-compatible Models Adapter

**Files:**

- Create or modify: `src-tauri/src/services/collectors/adapters/openai_compatible.rs`
- Modify: `src-tauri/src/services/collectors/apply.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/change_events.rs`

- [ ] **Step 1: Add model parser test**

Create `openai_compatible.rs`:

```rust
use serde_json::Value;

fn parse_openai_models(station_id: &str, payload: &Value) -> Vec<crate::services::collectors::facts::CollectedModelFact> {
    payload
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("id").and_then(Value::as_str))
        .map(|model| crate::services::collectors::facts::CollectedModelFact {
            station_id: station_id.to_string(),
            model: model.to_string(),
            available: true,
            source: "openai_models".to_string(),
            confidence: 0.9,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn openai_models_parser_reads_data_ids() {
        let models = parse_openai_models(
            "station-1",
            &json!({
                "data": [
                    { "id": "gpt-4o-mini" },
                    { "id": "text-embedding-3-small" }
                ]
            }),
        );

        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|model| model.model == "gpt-4o-mini"));
    }
}
```

- [ ] **Step 2: Implement HTTP models collection**

Add:

```rust
pub fn collect_models(database: &AppDatabase, data_key: &[u8; 32], station_id: &str) -> Result<AdapterOutput, String> {
    let station = database.station_for_collector(station_id)?;
    let keys = database.list_station_keys(station_id.to_string())?;
    let Some(key) = keys.into_iter().find(|key| key.enabled && key.api_key_present) else {
        return manual_required_output("openai-compatible", CollectorTask::Models, "api_key_required", "模型采集需要可用 API Key。");
    };
    let Some(api_key) = database.resolve_station_key_secret(key.id.clone(), data_key)? else {
        return manual_required_output("openai-compatible", CollectorTask::Models, "api_key_required", "API Key 不可解密。");
    };
    let urls = crate::services::collectors::url::collector_base_urls(&station.base_url);
    let url = crate::services::collectors::url::join_url(&urls.upstream_api_base_url, "/models");
    let response = match ureq::get(&url).set("Authorization", &format!("Bearer {api_key}")).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => {
            return Ok(AdapterOutput {
                adapter: "openai-compatible".to_string(),
                task: CollectorTask::Models,
                status: "failed".to_string(),
                facts: CollectorFacts::default(),
                summary_json: serde_json::json!({ "adapter": "openai-compatible", "task": "models" }),
                normalized_json: serde_json::json!({ "models": [] }),
                raw_json_redacted: None,
                error_code: Some("network_error".to_string()),
                error_message: Some(crate::services::secrets::mask::redact_text(&error.to_string())),
            });
        }
    };
    let text = response.into_string().unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&text).unwrap_or(Value::Null);
    let models = parse_openai_models(station_id, &payload);
    let mut facts = CollectorFacts::default();
    facts.models = models.clone();
    Ok(AdapterOutput {
        adapter: "openai-compatible".to_string(),
        task: CollectorTask::Models,
        status: if models.is_empty() { "partial" } else { "success" }.to_string(),
        summary_json: serde_json::json!({ "adapter": "openai-compatible", "task": "models", "modelCount": models.len() }),
        normalized_json: serde_json::json!({ "models": models.iter().map(|model| model.model.clone()).collect::<Vec<_>>() }),
        raw_json_redacted: Some(crate::services::secrets::mask::redact_value(&payload)),
        error_code: None,
        error_message: None,
        facts,
    })
}
```

- [ ] **Step 3: Extend apply layer for models**

When models are applied, insert a collector snapshot that contains normalized `models` and let existing or new change-event diff detect `model_added` and `model_removed`.

If current model diff lives in `insert_collector_snapshot`, keep that path and ensure the new adapter snapshots use the same normalized shape:

```json
{ "models": ["gpt-4o-mini", "text-embedding-3-small"] }
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml openai_models_parser_reads_data_ids --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 5: Commit**

Run:

```powershell
git add -- src-tauri/src/services/collectors/adapters/openai_compatible.rs src-tauri/src/services/collectors/apply.rs src-tauri/src/services/database.rs src-tauri/src/services/change_events.rs
git -c gc.auto=0 commit -m "feat: collect OpenAI compatible model facts"
```

---

## Task 13: Add Adapter Dispatch and Collector Commands

**Files:**

- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/lib/types/collector.ts`
- Modify: `src/lib/api/collector.ts`

- [ ] **Step 1: Add backend dispatch function**

In `collectors/mod.rs`, add:

```rust
pub fn collect_station_task(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: String,
    task: adapters::CollectorTask,
) -> Result<CollectorRunResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let output = match station.station_type.as_str() {
        "sub2api" => adapters::sub2api::collect(database, data_key, &station_id, task)?,
        "newapi" => adapters::newapi::collect(database, data_key, &station_id, task)?,
        "openai-compatible" | "custom" => adapters::openai_compatible::collect(database, data_key, &station_id, task)?,
        other => return Err(format!("不支持的站点类型: {other}")),
    };

    apply::apply_adapter_output(database, output)
}
```

Implement `apply_adapter_output` in `apply.rs` to:

1. create collector run;
2. insert collector snapshot;
3. apply facts;
4. finish run.

- [ ] **Step 2: Preserve existing collect command**

Keep existing `collect_station_info` command working by mapping it to full:

```rust
collect_station_task(database, data_key, station_id, adapters::CollectorTask::Full)
```

- [ ] **Step 3: Add new command**

In `commands/mod.rs`:

```rust
#[tauri::command]
pub async fn collect_station_task(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_id: String,
    task_type: String,
) -> Result<CollectorRunResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        let task = match task_type.as_str() {
            "detect" => collectors::adapters::CollectorTask::Detect,
            "balance" => collectors::adapters::CollectorTask::Balance,
            "groups" => collectors::adapters::CollectorTask::Groups,
            "models" => collectors::adapters::CollectorTask::Models,
            "full" => collectors::adapters::CollectorTask::Full,
            _ => return Err("未知采集任务类型".to_string()),
        };
        collectors::collect_station_task(&database, &data_key, station_id, task)
    })
    .await
    .map_err(|error| format!("采集任务执行失败: {error}"))?
}
```

Register in `lib.rs`.

- [ ] **Step 4: Add TypeScript task type**

Modify `src/lib/types/collector.ts`:

```ts
export type CollectorTaskType = "detect" | "balance" | "groups" | "models" | "full";
```

- [ ] **Step 5: Add API wrapper**

Modify `src/lib/api/collector.ts`:

```ts
export function collectStationTask(stationId: string, taskType: CollectorTaskType) {
  return invoke<CollectorRunResult>("collect_station_task", { stationId, taskType }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return createMemoryRun(stationId, `station-${taskType}`, "checked");
    }
    throw error;
  });
}
```

- [ ] **Step 6: Run checks**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm.cmd tsc --noEmit
```

Expected:

- PASS.

- [ ] **Step 7: Commit**

Run:

```powershell
git add -- src-tauri/src/services/collectors/mod.rs src-tauri/src/services/collectors/apply.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/lib/types/collector.ts src/lib/api/collector.ts
git -c gc.auto=0 commit -m "feat: dispatch collector tasks by adapter"
```

---

## Task 14: Extend Pricing Normalization and Route Economics

**Files:**

- Modify: `src-tauri/src/services/pricing/mod.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/models/routing.rs`
- Modify: `src-tauri/src/models/proxy.rs`

- [ ] **Step 1: Add pricing normalization tests**

In `pricing/mod.rs` tests add:

```rust
#[test]
fn group_rate_only_is_not_complete_pricing() {
    let input = UpsertPricingRuleInput {
        id: None,
        station_id: "station-1".to_string(),
        station_key_id: None,
        group_binding_id: Some("binding-1".to_string()),
        group_name: Some("default".to_string()),
        tier_label: None,
        model: "gpt-test".to_string(),
        input_price: None,
        output_price: None,
        fixed_price: None,
        currency: "USD".to_string(),
        unit: "multiplier".to_string(),
        price_type: "rate".to_string(),
        source: "collector".to_string(),
        confidence: 0.8,
        enabled: true,
        note: None,
        collected_at: None,
        rate_multiplier: Some(0.8),
        base_price_source: None,
        normalization_status: Some("group_rate_only".to_string()),
        valid_from: None,
        valid_until: None,
    };

    let sanitized = sanitize_pricing_rule_input(input);
    assert_eq!(sanitized.normalization_status.as_deref(), Some("group_rate_only"));
    assert!(sanitized.output_price.is_none());
}
```

- [ ] **Step 2: Extend route economics model**

Modify `RouteCandidateEconomics`:

```rust
pub group_binding_id: Option<String>,
pub rate_multiplier: Option<f64>,
pub normalization_status: Option<String>,
pub price_confidence: Option<f64>,
pub balance_scope: Option<String>,
pub balance_collected_at: Option<String>,
pub economic_freshness: Option<String>,
```

Modify `RouteCandidateExplanation` similarly.

- [ ] **Step 3: Update database economics query**

In `route_candidate_economics_from_connection`, include new pricing and balance fields. Query newest enabled pricing rule for station/key/model with `normalization_status = 'complete'` first. If only `group_rate_only` exists, return it with no complete price and include status.

Add rejection/penalty logic:

```rust
if economics.normalization_status.as_deref() == Some("group_rate_only") {
    reasons.push("only group rate is available; exact price unknown".to_string());
}
```

- [ ] **Step 4: Update cheap-first scoring**

In router scoring:

```rust
fn cheap_first_score(economics: &RouteCandidateEconomics) -> i64 {
    if economics.normalization_status.as_deref() != Some("complete") {
        return 50_000 + balance_penalty(economics);
    }
    let estimated_cost = estimated_cost(economics);
    (estimated_cost * 1_000_000.0).round() as i64 + balance_penalty(economics)
}
```

- [ ] **Step 5: Add balance depleted fallback setting**

Use `allow_depleted_fallback` from settings in route selection. If false and balance depleted, reject candidate:

```rust
if !settings.allow_depleted_fallback && economics.balance_status.as_deref() == Some("depleted") {
    rejection_reasons.push("balance depleted".to_string());
}
```

If passing settings into router is too wide for one task, add a `RouteRequest.allow_depleted_fallback: bool` field and populate it where route requests are built.

- [ ] **Step 6: Extend request log fields**

Add request log fields if not already present:

```rust
group_binding_id TEXT
normalization_status TEXT
balance_scope TEXT
economic_context_json TEXT
```

Persist economic context as redacted JSON:

```rust
serde_json::json!({
    "groupBindingId": selection.group_binding_id,
    "rateMultiplier": selection.rate_multiplier,
    "normalizationStatus": selection.normalization_status,
    "balanceScope": selection.balance_scope,
    "economicFreshness": selection.economic_freshness
})
```

- [ ] **Step 7: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml group_rate_only_is_not_complete_pricing --lib
cargo test --manifest-path .\src-tauri\Cargo.toml cheap_first --lib
cargo test --manifest-path .\src-tauri\Cargo.toml balance --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 8: Commit**

Run:

```powershell
git add -- src-tauri/src/services/pricing/mod.rs src-tauri/src/services/database.rs src-tauri/src/services/proxy/router.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/models/routing.rs src-tauri/src/models/proxy.rs
git -c gc.auto=0 commit -m "feat: use fact economics in routing"
```

---

## Task 15: Expand Change Events for P9 Facts

**Files:**

- Modify: `src-tauri/src/services/change_events.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src/features/changes/changeEventViewModels.ts`

- [ ] **Step 1: Add event builders**

In `change_events.rs`, add:

```rust
pub fn group_missing_event(station_id: &str, group_name: &str, group_binding_id: &str) -> UpsertChangeEventInput {
    UpsertChangeEventInput {
        severity: SEVERITY_WARNING.to_string(),
        event_type: "group_missing".to_string(),
        title: "分组不可见".to_string(),
        message: format!("分组 {group_name} 在最新采集中不可见"),
        object_type: "group_binding".to_string(),
        object_id: Some(group_binding_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json: None,
        impact_json: Some(serde_json::json!({ "routingRisk": "bound_keys_may_be_unavailable" }).to_string()),
        dedupe_key: format!("group_missing:station:{station_id}:group:{group_binding_id}"),
        source: "collector".to_string(),
    }
}

pub fn key_group_unresolved_event(station_id: &str, station_key_id: &str) -> UpsertChangeEventInput {
    UpsertChangeEventInput {
        severity: SEVERITY_WARNING.to_string(),
        event_type: "key_group_unresolved".to_string(),
        title: "Key 分组无法识别".to_string(),
        message: "采集器无法识别这把 Key 所属分组，需要手动绑定。".to_string(),
        object_type: "station_key".to_string(),
        object_id: Some(station_key_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: Some(station_key_id.to_string()),
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json: None,
        impact_json: Some(serde_json::json!({ "cheapFirstConfidence": "reduced" }).to_string()),
        dedupe_key: format!("key_group_unresolved:key:{station_key_id}"),
        source: "collector".to_string(),
    }
}
```

Add builders for:

- `collector_recovered`;
- `price_expired`;
- `route_impacted`;
- `key_group_bound`.

- [ ] **Step 2: Emit events from apply/database**

Trigger:

- group newly available -> `group_added` info;
- previously available now missing -> `group_missing` warning;
- key auto-bound -> `key_group_bound` info;
- key unresolved -> `key_group_unresolved` warning;
- collector failed -> existing `collector_failed`;
- collector failed then success -> `collector_recovered`.

- [ ] **Step 3: Update frontend labels**

Modify `changeEventViewModels.ts`:

```ts
group_added: "分组新增",
group_missing: "分组不可见",
key_group_bound: "Key 分组已绑定",
key_group_unresolved: "Key 分组无法识别",
price_expired: "价格过期",
route_impacted: "路由受影响",
collector_recovered: "采集恢复",
```

Add object type labels:

```ts
group_binding: "分组绑定",
collector_run: "采集任务",
```

- [ ] **Step 4: Run tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml group_missing --lib
cargo test --manifest-path .\src-tauri\Cargo.toml key_group_unresolved --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm.cmd tsc --noEmit
```

Expected:

- PASS.

- [ ] **Step 5: Commit**

Run:

```powershell
git add -- src-tauri/src/services/change_events.rs src-tauri/src/services/database.rs src/features/changes/changeEventViewModels.ts
git -c gc.auto=0 commit -m "feat: track P9 fact change events"
```

---

## Task 16: Add Frontend Types and API Wrappers

**Files:**

- Create: `src/lib/types/groupFacts.ts`
- Create: `src/lib/types/collectorRuns.ts`
- Modify: `src/lib/types/economics.ts`
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/lib/types/settings.ts`
- Modify: `src/lib/types/routing.ts`
- Modify: `src/lib/types/proxy.ts`
- Create: `src/lib/api/groupFacts.ts`
- Create: `src/lib/api/collectorRuns.ts`
- Modify: `src/lib/api/economics.ts`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src/lib/api/settings.ts`
- Modify: `src/lib/mock/settings.ts`

- [ ] **Step 1: Add group fact types**

Create `src/lib/types/groupFacts.ts`:

```ts
export type BindingKind = "station_group" | "key_binding" | string;
export type BindingStatus = "available" | "bound" | "missing" | "disabled" | "manual_legacy" | string;

export type StationGroupBinding = {
  id: string;
  stationId: string;
  stationKeyId: string | null;
  bindingKind: BindingKind;
  parentGroupBindingId: string | null;
  groupKeyHash: string;
  groupIdHash: string | null;
  groupName: string;
  bindingStatus: BindingStatus;
  defaultRateMultiplier: number | null;
  userRateMultiplier: number | null;
  effectiveRateMultiplier: number | null;
  rateSource: string | null;
  confidence: number;
  lastSeenAt: string | null;
  lastCheckedAt: string | null;
  lastRateChangedAt: string | null;
  rawJsonRedacted: Record<string, unknown> | null;
  createdAt: string;
  updatedAt: string;
};

export type GroupRateRecord = {
  id: string;
  stationId: string;
  stationKeyId: string | null;
  groupBindingId: string | null;
  bindingKind: BindingKind;
  groupKeyHash: string;
  groupName: string;
  defaultRateMultiplier: number | null;
  userRateMultiplier: number | null;
  effectiveRateMultiplier: number | null;
  source: string;
  confidence: number;
  rawJsonRedacted: Record<string, unknown> | null;
  checkedAt: string;
  createdAt: string;
};
```

- [ ] **Step 2: Add collector run types**

Create `src/lib/types/collectorRuns.ts`:

```ts
export type CollectorTaskType = "detect" | "balance" | "groups" | "models" | "full";
export type CollectorRunStatus = "running" | "success" | "partial" | "failed" | "manual_required" | string;

export type CollectorRun = {
  id: string;
  stationId: string;
  parentRunId: string | null;
  adapter: string;
  taskType: CollectorTaskType | string;
  status: CollectorRunStatus;
  startedAt: string;
  finishedAt: string | null;
  durationMs: number | null;
  endpointCount: number;
  successCount: number;
  failureCount: number;
  manualActionRequired: boolean;
  errorCode: string | null;
  errorMessage: string | null;
  snapshotId: string | null;
  createdAt: string;
};
```

- [ ] **Step 3: Add API wrappers**

Create `src/lib/api/groupFacts.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { isInvokeUnavailable } from "@/lib/api/errors";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";

const memoryBindings = new Map<string, StationGroupBinding[]>();
const memoryRates = new Map<string, GroupRateRecord[]>();

export function listStationGroupBindings(stationId: string) {
  return invoke<StationGroupBinding[]>("list_station_group_bindings", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) return memoryBindings.get(stationId) ?? [];
    throw error;
  });
}

export function listGroupRateRecords(stationId: string) {
  return invoke<GroupRateRecord[]>("list_group_rate_records", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) return memoryRates.get(stationId) ?? [];
    throw error;
  });
}
```

Create `src/lib/api/collectorRuns.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import { isInvokeUnavailable } from "@/lib/api/errors";
import type { CollectorRun } from "@/lib/types/collectorRuns";

export function listCollectorRuns(stationId: string) {
  return invoke<CollectorRun[]>("list_collector_runs", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) return [];
    throw error;
  });
}
```

If this repo does not have `src/lib/api/errors.ts`, use the local `isInvokeUnavailable` helper pattern from existing API modules rather than introducing a new shared helper in this task.

- [ ] **Step 4: Extend existing types**

Update:

- `PricingRule`: add `stationKeyId`, `groupBindingId`, `rateMultiplier`, `basePriceSource`, `normalizationStatus`, `validFrom`, `validUntil`.
- `BalanceSnapshot`: no P9 field required unless backend changed.
- `StationKey` and `KeyPoolItem`: add group binding/rate/balance scope fields.
- `AppSettings`: add P9 settings.
- `RouteCandidateExplanation`: add group/rate/pricing/balance freshness fields.
- `RequestLog`: add economic context fields if backend added them.

- [ ] **Step 5: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
```

Expected:

- TypeScript compile passes.

- [ ] **Step 6: Commit**

Run:

```powershell
git add -- src/lib/types/groupFacts.ts src/lib/types/collectorRuns.ts src/lib/types/economics.ts src/lib/types/stationKeys.ts src/lib/types/settings.ts src/lib/types/routing.ts src/lib/types/proxy.ts src/lib/api/groupFacts.ts src/lib/api/collectorRuns.ts src/lib/api/economics.ts src/lib/api/stationKeys.ts src/lib/api/settings.ts src/lib/mock/settings.ts
git -c gc.auto=0 commit -m "feat: add P9 frontend fact APIs"
```

Do not stage files that did not change.

---

## Task 17: Upgrade Collector Center for Task Runs and Manual Session

**Files:**

- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/lib/api/collector.ts`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src/lib/types/stationKeys.ts`

- [ ] **Step 1: Add task selector state**

In `CollectorsPage.tsx`, add:

```tsx
import type { CollectorTaskType } from "@/lib/types/collectorRuns";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
import { collectStationTask } from "@/lib/api/collector";
```

Add state:

```tsx
const [taskType, setTaskType] = useState<CollectorTaskType>("full");
const [runs, setRuns] = useState<CollectorRun[]>([]);
const [manualSession, setManualSession] = useState({
  accessToken: "",
  refreshToken: "",
  cookie: "",
  newapiUserId: "",
  tokenExpiresAt: "",
});
```

- [ ] **Step 2: Load collector runs**

In selected station effect, add:

```tsx
void refreshRuns(selectedStation.id);
```

Implement:

```tsx
async function refreshRuns(stationId: string) {
  try {
    setRuns(await listCollectorRuns(stationId));
  } catch (requestError) {
    toast.error("刷新采集任务失败", readError(requestError));
  }
}
```

- [ ] **Step 3: Replace collect handler**

Change `handleCollect` to:

```tsx
async function handleCollect() {
  if (!selectedStation) return;
  setTaskStatus("collecting");
  setError(null);
  try {
    const result = await collectStationTask(selectedStation.id, taskType);
    setLatestSnapshot(result.snapshot);
    await Promise.all([
      refreshStations(),
      refreshSnapshot(selectedStation.id),
      refreshRuns(selectedStation.id),
    ]);
    setTaskStatus("success");
    toast.success("采集任务已完成");
  } catch (requestError) {
    setTaskStatus("failed");
    toast.error("采集任务失败", shortError(readError(requestError)));
  }
}
```

- [ ] **Step 4: Add task selector UI**

Near action buttons render:

```tsx
<SelectControl
  ariaLabel="采集任务"
  value={taskType}
  options={[
    { value: "detect", label: "Detect" },
    { value: "balance", label: "余额" },
    { value: "groups", label: "分组 / 倍率" },
    { value: "models", label: "模型" },
    { value: "full", label: "Full" },
  ]}
  onChange={(value) => setTaskType(value as CollectorTaskType)}
/>
```

- [ ] **Step 5: Add runs panel**

Render a compact table:

```tsx
<SectionCard title="最近采集任务" description="父任务 full 会展开为 balance / groups / models 子任务。">
  <div className="grid gap-2">
    {runs.length === 0 ? (
      <div className="text-sm text-muted-foreground">暂无采集任务。</div>
    ) : (
      runs.slice(0, 10).map((run) => (
        <div key={run.id} className="grid grid-cols-[5rem_7rem_1fr_5rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 text-xs">
          <span className="font-medium text-slate-700">{run.taskType}</span>
          <StatusBadge tone={run.status === "success" ? "success" : run.status === "failed" ? "error" : run.status === "manual_required" ? "warning" : "info"}>
            {run.status}
          </StatusBadge>
          <span className="truncate text-muted-foreground">{run.errorMessage ?? `${run.successCount}/${run.endpointCount} endpoint`}</span>
          <span className="text-right text-muted-foreground">{run.durationMs == null ? "-" : `${run.durationMs}ms`}</span>
        </div>
      ))
    )}
  </div>
</SectionCard>
```

- [ ] **Step 6: Add manual session form**

Add a developer-only section on the same page:

```tsx
<SectionCard title="手动登录态" description="保存 access token、refresh token、NewAPI User ID 或 cookie；保存后不会回显原文。">
  <div className="grid gap-3 md:grid-cols-2">
    <Field label="Access Token">
      <input type="password" className={inputClassName} value={manualSession.accessToken} onChange={(event) => setManualSession({ ...manualSession, accessToken: event.target.value })} />
    </Field>
    <Field label="Refresh Token">
      <input type="password" className={inputClassName} value={manualSession.refreshToken} onChange={(event) => setManualSession({ ...manualSession, refreshToken: event.target.value })} />
    </Field>
    <Field label="NewAPI User ID">
      <input className={inputClassName} value={manualSession.newapiUserId} onChange={(event) => setManualSession({ ...manualSession, newapiUserId: event.target.value })} />
    </Field>
    <Field label="Cookie">
      <input type="password" className={inputClassName} value={manualSession.cookie} onChange={(event) => setManualSession({ ...manualSession, cookie: event.target.value })} />
    </Field>
  </div>
</SectionCard>
```

Wire save button to `updateStationSession`. After saving, clear token fields:

```tsx
setManualSession({ accessToken: "", refreshToken: "", cookie: "", newapiUserId: "", tokenExpiresAt: "" });
```

- [ ] **Step 7: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 8: Commit**

Run:

```powershell
git add -- src/features/collectors/CollectorsPage.tsx src/lib/api/collector.ts src/lib/api/stationKeys.ts src/lib/types/stationKeys.ts
git -c gc.auto=0 commit -m "feat: upgrade collector center task runs"
```

---

## Task 18: Upgrade Station Asset Drawer to Use Durable Facts

**Files:**

- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/stations/stationAssetViewModels.ts`
- Modify: `src/features/stations/components/StationDetailPanel.tsx`

- [ ] **Step 1: Load group facts and runs**

In `StationsPage.tsx`, import:

```tsx
import { listStationGroupBindings, listGroupRateRecords } from "@/lib/api/groupFacts";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
```

Add state maps:

```tsx
const [groupBindingsByStation, setGroupBindingsByStation] = useState(new Map<string, StationGroupBinding[]>());
const [rateRecordsByStation, setRateRecordsByStation] = useState(new Map<string, GroupRateRecord[]>());
const [collectorRunsByStation, setCollectorRunsByStation] = useState(new Map<string, CollectorRun[]>());
```

- [ ] **Step 2: Refresh facts per station**

Add:

```tsx
async function refreshStationFacts(stationId: string) {
  const [bindings, rates, runs] = await Promise.all([
    listStationGroupBindings(stationId),
    listGroupRateRecords(stationId),
    listCollectorRuns(stationId),
  ]);
  setGroupBindingsByStation((current) => new Map(current).set(stationId, bindings));
  setRateRecordsByStation((current) => new Map(current).set(stationId, rates));
  setCollectorRunsByStation((current) => new Map(current).set(stationId, runs));
}
```

Call it when opening drawer and after collecting.

- [ ] **Step 3: Update view model rate chips**

In `stationAssetViewModels.ts`, prefer bindings:

```ts
export function rateChipsFromBindings(bindings: StationGroupBinding[]) {
  return bindings
    .filter((binding) => binding.bindingKind === "station_group")
    .slice(0, 3)
    .map((binding) => ({
      label: binding.groupName,
      value: binding.effectiveRateMultiplier ?? binding.defaultRateMultiplier ?? null,
      tone: binding.bindingStatus === "missing" ? "warning" : "neutral",
    }));
}
```

Keep snapshot fallback only when no durable bindings exist.

- [ ] **Step 4: Add drawer sections**

In drawer body render:

- `Group bindings`
- `Rate history`
- `Collector runs`

Use compact rows:

```tsx
<div className="grid grid-cols-[1fr_5rem_6rem_7rem] items-center gap-2 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 text-xs">
  <span className="truncate font-medium text-slate-700">{binding.groupName}</span>
  <span>{binding.effectiveRateMultiplier ?? "-"}</span>
  <StatusBadge tone={binding.bindingStatus === "missing" ? "warning" : "info"}>{binding.bindingStatus}</StatusBadge>
  <span className="truncate text-muted-foreground">{binding.rateSource ?? "unknown"}</span>
</div>
```

- [ ] **Step 5: Keep main table dense**

Verify main station asset list still shows only:

- name/type/base URL;
- balance;
- up to 3 rate chips;
- key count;
- collection status;
- health;
- updated time;
- routing participation;
- collect/detail actions.

- [ ] **Step 6: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 7: Commit**

Run:

```powershell
git add -- src/features/stations/StationsPage.tsx src/features/stations/stationAssetViewModels.ts src/features/stations/components/StationDetailPanel.tsx
git -c gc.auto=0 commit -m "feat: show durable facts in station assets"
```

---

## Task 19: Upgrade Key Pool for Group Binding and Routeability

**Files:**

- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Include fact fields in key pool query**

In `list_key_pool_items_from_connection`, include `station_keys.group_binding_id`, `rate_multiplier`, `rate_source`, `rate_collected_at`, `balance_scope`, and join `station_group_bindings` for binding status and group name where available.

Return values through `KeyPoolItem`.

- [ ] **Step 2: Add manual bind command**

In backend command layer:

```rust
#[tauri::command]
pub fn update_station_key_group_binding(
    database: State<'_, AppDatabase>,
    input: UpdateStationKeyGroupBindingInput,
) -> Result<StationKey, String> {
    database.update_station_key_group_binding(input)
}
```

The database method updates:

```sql
UPDATE station_keys
SET group_binding_id = ?1,
    group_id_hash = ?2,
    rate_multiplier = ?3,
    rate_source = 'manual',
    rate_collected_at = ?4,
    updated_at = ?4
WHERE id = ?5
```

Use the selected binding's `group_key_hash` and `effective_rate_multiplier`.

- [ ] **Step 3: Add frontend binding API**

In `stationKeys.ts`:

```ts
export function updateStationKeyGroupBinding(stationKeyId: string, groupBindingId: string) {
  return invoke<StationKey>("update_station_key_group_binding", {
    input: { stationKeyId, groupBindingId },
  });
}
```

- [ ] **Step 4: Add key pool columns**

In `KeyPoolPage.tsx`, add columns or compact row fields:

- group name;
- binding status;
- effective multiplier;
- balance scope;
- routeability;
- price status.

Render group status:

```tsx
<StatusBadge tone={item.groupBindingId ? "success" : "warning"}>
  {item.groupBindingId ? "已绑定" : "待绑定"}
</StatusBadge>
```

- [ ] **Step 5: Add binding selector**

In key detail/edit drawer, load station group bindings for the key's station and render:

```tsx
<SelectControl
  ariaLabel="绑定分组"
  value={keyForm.groupBindingId ?? ""}
  options={[
    { value: "", label: "未绑定" },
    ...bindings
      .filter((binding) => binding.bindingKind === "station_group")
      .map((binding) => ({ value: binding.id, label: `${binding.groupName} · ${binding.effectiveRateMultiplier ?? "-"}` })),
  ]}
  onChange={(groupBindingId) => setKeyForm({ ...keyForm, groupBindingId: groupBindingId || null })}
/>
```

- [ ] **Step 6: Run checks**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 7: Commit**

Run:

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/features/key-pool/KeyPoolPage.tsx src/lib/api/stationKeys.ts src/lib/types/stationKeys.ts
git -c gc.auto=0 commit -m "feat: bind key pool items to groups"
```

---

## Task 20: Upgrade Price and Rate Matrix to Durable Facts

**Files:**

- Modify: `src/features/pricing/PricingPage.tsx`
- Modify: `src/features/pricing/pricingMatrix.ts`
- Modify: `src/features/pricing/rateSnapshotParser.ts`
- Modify: `src/lib/api/economics.ts`

- [ ] **Step 1: Replace snapshot-derived rates**

In `pricingMatrix.ts`, replace `RateMultiplierRow` dependency with `GroupRateRecord` or `StationGroupBinding`:

```ts
export function buildRateMatrixFromBindings(bindings: StationGroupBinding[], stations: Station[]): RateMatrixRow[] {
  const groupNames = Array.from(new Set(bindings.map((binding) => binding.groupName))).sort((a, b) => a.localeCompare(b));
  return groupNames.map((groupName) => ({
    groupName,
    cells: stations.map((station) => {
      const binding = newestBinding(bindings.filter((item) => item.stationId === station.id && item.groupName === groupName));
      return {
        stationId: station.id,
        multiplier: binding?.effectiveRateMultiplier ?? null,
        updatedAt: binding?.updatedAt ?? "",
        source: binding?.rateSource ?? "",
        status: binding?.bindingStatus ?? "unavailable",
      };
    }),
  }));
}
```

- [ ] **Step 2: Add price cell normalization status**

Extend `PriceMatrixCell`:

```ts
normalizationStatus: string;
rateMultiplier: number | null;
groupBindingId: string | null;
source: string;
confidence: number;
```

When rendering a cell, show status:

```tsx
<StatusBadge tone={cell.normalizationStatus === "complete" ? "success" : cell.normalizationStatus === "group_rate_only" ? "warning" : "info"}>
  {cell.normalizationStatus}
</StatusBadge>
```

- [ ] **Step 3: Load group facts**

In `PricingPage.tsx`, load all station bindings:

```tsx
const bindingLists = await Promise.all(nextStations.map((station) => listStationGroupBindings(station.id)));
setGroupBindings(bindingLists.flat());
```

Also load rate records if the detail drawer needs history.

- [ ] **Step 4: Keep three matrix tabs**

Ensure the page still has:

- model price matrix;
- group rate matrix;
- availability matrix.

Availability may still use pricing/model data if no dedicated model facts API exists yet, but label unavailable/unknown honestly.

- [ ] **Step 5: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 6: Commit**

Run:

```powershell
git add -- src/features/pricing/PricingPage.tsx src/features/pricing/pricingMatrix.ts src/features/pricing/rateSnapshotParser.ts src/lib/api/economics.ts
git -c gc.auto=0 commit -m "feat: use durable facts in price rate matrix"
```

---

## Task 21: Upgrade Routing and Request Log UI

**Files:**

- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/lib/types/routing.ts`
- Modify: `src/lib/types/proxy.ts`

- [ ] **Step 1: Render route economics**

In `RoutingPage.tsx`, for each explanation render:

```tsx
<div className="grid gap-1 text-xs text-muted-foreground">
  <div>Group: {candidate.groupBindingId ?? "未绑定"}</div>
  <div>Rate: {candidate.rateMultiplier ?? "未知"}</div>
  <div>Pricing: {candidate.normalizationStatus ?? "unknown"}</div>
  <div>Balance: {candidate.balanceStatus ?? "unknown"} · {candidate.balanceScope ?? "unknown"}</div>
  <div>Freshness: {candidate.economicFreshness ?? "unknown"}</div>
</div>
```

- [ ] **Step 2: Show rejected reasons clearly**

For rejected candidates, include:

```tsx
{candidate.rejectionReasons.map((reason) => (
  <StatusBadge key={reason} tone="warning">{reason}</StatusBadge>
))}
```

Ensure `group_rate_only` appears as explanation, not as precise low price.

- [ ] **Step 3: Render request economic context**

In `LogsPage.tsx`, add fields:

- pricing rule id;
- group binding id;
- normalization status;
- balance status;
- fallback count;
- rejected candidates.

Use compact inspector rows:

```tsx
<PropertyRow label="价格状态" value={log.normalizationStatus ?? log.costStatus ?? "unknown"} />
<PropertyRow label="Group Binding" value={log.groupBindingId ?? "未记录"} />
<PropertyRow label="余额作用域" value={log.balanceScope ?? "unknown"} />
```

- [ ] **Step 4: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 5: Commit**

Run:

```powershell
git add -- src/features/routing/RoutingPage.tsx src/features/logs/LogsPage.tsx src/lib/types/routing.ts src/lib/types/proxy.ts
git -c gc.auto=0 commit -m "feat: explain P9 route economics in UI"
```

---

## Task 22: Upgrade Change Center for P9 Events

**Files:**

- Modify: `src/features/changes/ChangeCenterPage.tsx`
- Modify: `src/features/changes/changeEventViewModels.ts`
- Modify: `src/components/shell/AppShell.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`

- [ ] **Step 1: Add object filter options**

In `changeEventViewModels.ts`, add:

```ts
export const changeObjectTypeLabels: Record<string, string> = {
  station: "中转站",
  station_key: "Key",
  group_binding: "分组",
  pricing_rule: "价格",
  collector_run: "采集",
  request_log: "请求",
  route: "路由",
};
```

- [ ] **Step 2: Add event labels**

Add labels for:

```ts
group_added: "分组新增",
group_missing: "分组不可见",
key_group_bound: "Key 分组已绑定",
key_group_unresolved: "Key 分组无法识别",
price_expired: "价格过期",
route_impacted: "路由受影响",
collector_recovered: "采集恢复",
```

- [ ] **Step 3: Add detail old/new/impact renderer**

In `ChangeCenterPage.tsx`, render:

```tsx
<JsonPreview title="旧值" value={selected.oldValueJson} />
<JsonPreview title="新值" value={selected.newValueJson} />
<JsonPreview title="影响" value={selected.impactJson} />
```

Implement:

```tsx
function JsonPreview({ title, value }: { title: string; value: string | null }) {
  if (!value) return null;
  return (
    <section className="grid gap-2">
      <div className="text-xs font-semibold text-slate-700">{title}</div>
      <pre className="max-h-40 overflow-auto rounded-[var(--surface-radius)] border border-border bg-slate-50 p-2 text-xs text-slate-700">
        {formatJson(value)}
      </pre>
    </section>
  );
}

function formatJson(value: string) {
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}
```

- [ ] **Step 4: Update dashboard risk summary**

In `DashboardPage.tsx`, add counters:

- unresolved critical events;
- group missing/key unresolved;
- collector failed;
- price expired/rate increased.

Do not show full tables on dashboard.

- [ ] **Step 5: Confirm sidebar badge remains risk-only**

In `AppShell.tsx`, ensure badge counts only unread critical/warning, not info.

- [ ] **Step 6: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 7: Commit**

Run:

```powershell
git add -- src/features/changes/ChangeCenterPage.tsx src/features/changes/changeEventViewModels.ts src/components/shell/AppShell.tsx src/features/dashboard/DashboardPage.tsx
git -c gc.auto=0 commit -m "feat: upgrade change center for P9 events"
```

---

## Task 23: Upgrade Settings for P9 Collection and Routing Controls

**Files:**

- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/lib/types/settings.ts`
- Modify: `src/lib/api/settings.ts`
- Modify: `src/lib/mock/settings.ts`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/models/settings.rs`

- [ ] **Step 1: Add frontend settings fields**

In `src/lib/types/settings.ts`, add:

```ts
balanceIntervalMinutes: number;
groupRateIntervalMinutes: number;
modelListIntervalMinutes: number;
pricingRefreshIntervalMinutes: number;
collectorTimeoutSeconds: number;
collectorMaxConcurrency: number;
allowDepletedFallback: boolean;
```

Add the same fields to `UpdateSettingsInput`.

- [ ] **Step 2: Parse settings in backend**

In `settings_from_connection`, parse:

```rust
balance_interval_minutes: get_u16_setting(connection, "balance_interval_minutes", 5)?,
group_rate_interval_minutes: get_u16_setting(connection, "group_rate_interval_minutes", 20)?,
model_list_interval_minutes: get_u16_setting(connection, "model_list_interval_minutes", 60)?,
pricing_refresh_interval_minutes: get_u16_setting(connection, "pricing_refresh_interval_minutes", 60)?,
collector_timeout_seconds: get_u16_setting(connection, "collector_timeout_seconds", 15)?,
collector_max_concurrency: get_u16_setting(connection, "collector_max_concurrency", 3)?,
allow_depleted_fallback: get_bool_setting(connection, "allow_depleted_fallback", false)?,
```

- [ ] **Step 3: Persist settings**

In `update_settings`, validate:

```rust
if input.balance_interval_minutes == 0 || input.group_rate_interval_minutes == 0 || input.model_list_interval_minutes == 0 {
    return Err("采集周期必须大于 0".to_string());
}
if input.collector_timeout_seconds < 3 {
    return Err("采集超时时间不能小于 3 秒".to_string());
}
if input.collector_max_concurrency == 0 || input.collector_max_concurrency > 8 {
    return Err("采集并发数必须在 1 到 8 之间".to_string());
}
```

Then write each key through existing settings update helper.

- [ ] **Step 4: Add UI controls**

In `SettingsPage.tsx`, add a `采集与路由` section:

```tsx
<SectionCard title="采集与路由" description="控制后台采集频率、超时和余额耗尽兜底策略。">
  <SettingsNumberField label="余额采集周期" suffix="分钟" value={form.balanceIntervalMinutes} onChange={(value) => setForm({ ...form, balanceIntervalMinutes: value })} />
  <SettingsNumberField label="分组 / 倍率采集周期" suffix="分钟" value={form.groupRateIntervalMinutes} onChange={(value) => setForm({ ...form, groupRateIntervalMinutes: value })} />
  <SettingsNumberField label="模型采集周期" suffix="分钟" value={form.modelListIntervalMinutes} onChange={(value) => setForm({ ...form, modelListIntervalMinutes: value })} />
  <SettingsNumberField label="采集超时" suffix="秒" value={form.collectorTimeoutSeconds} onChange={(value) => setForm({ ...form, collectorTimeoutSeconds: value })} />
  <SwitchControl checked={form.allowDepletedFallback} onCheckedChange={(allowDepletedFallback) => setForm({ ...form, allowDepletedFallback })} label="允许余额耗尽兜底" description="关闭时，余额耗尽的候选默认不参与路由。" />
</SectionCard>
```

Use existing input primitives if `SettingsNumberField` does not exist; do not introduce a large new component for this.

- [ ] **Step 5: Run checks**

Run:

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 6: Commit**

Run:

```powershell
git add -- src/features/settings/SettingsPage.tsx src/lib/types/settings.ts src/lib/api/settings.ts src/lib/mock/settings.ts src-tauri/src/services/database.rs src-tauri/src/models/settings.rs
git -c gc.auto=0 commit -m "feat: add P9 collection settings"
```

---

## Task 24: Update Docs and Product Model

**Files:**

- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/PRODUCT_MODEL.md`
- Modify: `docs/superpowers/specs/2026-07-03-p9-real-station-collection-routing-facts-design.md` only if implementation changed the design.
- Modify: `docs/superpowers/plans/2026-07-03-p9-real-station-collection-routing-facts.md`

- [ ] **Step 1: Update project plan phase list**

In `docs/PROJECT_PLAN.md`, add:

```markdown
- P9 真实站点采集与路由事实层：补齐 Sub2API / NewAPI / OpenAI-compatible adapter，建立 group binding、倍率历史、collector run、价格归一化和路由经济解释，让 UI 和路由消费稳定事实而不是 raw snapshot JSON。
```

- [ ] **Step 2: Update product model**

In `docs/PRODUCT_MODEL.md`, add:

```markdown
## Group Binding

`Group Binding` is the durable relationship between a Station, its available groups, and the Station Keys that route through those groups.

It owns:

- station-level group identity
- key-level binding identity
- group key hash
- optional external group id hash
- binding status
- rate source
- effective multiplier
- confidence
- latest visibility state

## Collector Run

`Collector Run` is a task-level record for detect, balance, groups, models, and full collection.

It owns:

- adapter
- task type
- parent run
- status
- endpoint counts
- duration
- manual action requirement
- linked collector snapshot
```

- [ ] **Step 3: Update completion notes**

If implementation deviates from spec, add a short note under the P9 spec:

```markdown
## Implementation Notes

- The implemented path keeps `collector_snapshots` as debug/audit storage and routes UI/business logic through P9 fact tables.
```

Do not add this section if there is no deviation or clarification needed.

- [ ] **Step 4: Review docs diff**

Run:

```powershell
git diff -- docs/PROJECT_PLAN.md docs/PRODUCT_MODEL.md docs/superpowers/specs/2026-07-03-p9-real-station-collection-routing-facts-design.md docs/superpowers/plans/2026-07-03-p9-real-station-collection-routing-facts.md
```

Expected:

- Diffs are documentation only.
- No unrelated docs churn.

- [ ] **Step 5: Commit**

Run:

```powershell
git add -- docs/PROJECT_PLAN.md docs/PRODUCT_MODEL.md docs/superpowers/plans/2026-07-03-p9-real-station-collection-routing-facts.md
git -c gc.auto=0 commit -m "docs: document P9 facts implementation"
```

Add the spec file only if it changed.

---

## Task 25: End-to-End Verification and Manual Smoke

**Files:**

- No planned edits unless verification exposes bugs.

- [ ] **Step 1: Run full automated checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected:

- All commands exit 0.
- Existing warnings may remain if already present; no new failing tests.

- [ ] **Step 2: Start the app**

Run:

```powershell
pnpm.cmd tauri:dev
```

Expected:

- App launches.
- Database migrations complete.
- Existing stations and keys still appear.

- [ ] **Step 3: Smoke Settings**

Open 设置.

Expected:

- Developer mode switch still works.
- New collection settings render and save:
  - balance interval;
  - group/rate interval;
  - model interval;
  - collector timeout;
  - max concurrency;
  - allow depleted fallback.

- [ ] **Step 4: Smoke Collector Center**

Enable developer mode and open 采集中心.

Run tasks:

- `detect`;
- `balance`;
- `groups`;
- `models`;
- `full`.

Expected:

- Each task creates a collector run.
- `full` creates a parent run and child runs.
- Manual token fields clear after save and do not echo secrets.
- Redacted raw does not show tokens/cookies/passwords.

- [ ] **Step 5: Smoke Sub2API with controlled or mock station**

Use a test Sub2API-compatible endpoint or local test server.

Expected:

- `/usage` creates `BalanceSnapshot`.
- groups/rates create `station_group_bindings`.
- rate changes create `group_rate_records`.
- Key binding is automatic when plan/group or key endpoint can resolve it.
- If unresolved, change center shows `key_group_unresolved`.

- [ ] **Step 6: Smoke NewAPI with controlled or mock station**

Use access token + user id.

Expected:

- `/api/user/self` quota converts by `/500000`.
- `/api/user/self/groups` creates group/rate facts.
- No password-login assumption is required.

- [ ] **Step 7: Smoke Station Assets**

Open 中转站资产.

Expected:

- Main table remains dense.
- Rate chips use durable bindings.
- Drawer shows group bindings, rate history, collector runs, related changes, keys, and balance.

- [ ] **Step 8: Smoke Key Pool**

Open Key 池.

Expected:

- Key rows show binding status, multiplier, balance scope, routeability, price state.
- Manual group binding saves and survives refresh.
- Missing group shows warning instead of silently disappearing.

- [ ] **Step 9: Smoke Price / Rate**

Open 价格 / 倍率.

Expected:

- Model price matrix shows normalization status.
- Rate matrix uses group bindings/rate records.
- `group_rate_only` is visible as status and not shown as exact model price.
- Availability matrix does not pretend unknown model data is confirmed.

- [ ] **Step 10: Smoke Routing**

Open 路由规则 and run route simulation.

Expected:

- Route explain includes capability, health, cooldown, group binding, rate multiplier, pricing rule, normalization status, balance status, and rejected candidates.
- `depleted` candidates are rejected unless `allow_depleted_fallback` is enabled.

- [ ] **Step 11: Smoke Request Logs**

Send a request through local proxy.

Expected:

- Request log includes station/key, pricing rule, group binding, normalization status, balance status, fallback count, and rejected candidates.
- No prompt, response body, token, cookie, password, or full key is persisted.

- [ ] **Step 12: Smoke Change Center and Dashboard**

Open 变更中心 and 总览.

Expected:

- Change center can filter P9 events.
- Detail drawer displays old/new/impact JSON.
- Sidebar badge counts unread critical/warning only.
- Dashboard shows risk summary, not raw full data.

- [ ] **Step 13: Run safety scan**

Use Settings security scan or command.

Expected:

- No full API key, password, access token, refresh token, cookie, or Authorization header appears in new P9 tables.

- [ ] **Step 14: Final status**

Run:

```powershell
git status --short -- .
git log --oneline -12
```

Expected:

- No unintended dirty files.
- Recent commits are concern-based.

- [ ] **Step 15: Final report**

Report:

- commit list;
- automated checks;
- manual smoke evidence;
- real or mock station compatibility used;
- any remaining known limitations;
- whether ready to merge/push.

---

## Suggested Commit Sequence

1. `feat: add P9 collection fact models`
2. `feat: add P9 fact schema migrations`
3. `feat: add P9 fact database operations`
4. `feat: migrate legacy group facts`
5. `feat: add collector facts adapter framework`
6. `feat: add collector run lifecycle`
7. `feat: collect Sub2API usage balance facts`
8. `feat: add collector session credential storage`
9. `feat: collect Sub2API group rate facts`
10. `feat: add NewAPI collector adapter`
11. `feat: collect OpenAI compatible model facts`
12. `feat: dispatch collector tasks by adapter`
13. `feat: use fact economics in routing`
14. `feat: track P9 fact change events`
15. `feat: add P9 frontend fact APIs`
16. `feat: upgrade collector center task runs`
17. `feat: show durable facts in station assets`
18. `feat: bind key pool items to groups`
19. `feat: use durable facts in price rate matrix`
20. `feat: explain P9 route economics in UI`
21. `feat: upgrade change center for P9 events`
22. `feat: add P9 collection settings`
23. `docs: document P9 facts implementation`

Use exact staging paths for every commit.

---

## P9 Pass/Fail Rubric

P9 passes if these statements are true:

```text
Sub2API 余额从 routeable Station Key 采集，不再依赖 Station 当作路由 key。
Sub2API 管理接口通过 management_base_url 调用，不会拼出 /v1/api/...。
NewAPI 是一等 adapter，能用 access token + user id 采余额和分组。
OpenAI-compatible 只承诺模型采集，不假装有余额和倍率。
Group Binding 区分站点 group 当前事实和 key binding。
group_key_hash 非空，缺外部 group id 时仍能稳定去重。
group_rate_records 只在倍率或名称变化时插入。
collector_runs 能表达 full 父任务和子任务。
collector_snapshots 只作为脱敏调试/审计，不再是 UI 和路由主事实源。
cheap_first 只用 complete pricing 做精确排序。
group_rate_only 不参与精确 cheap-first。
余额耗尽默认拒绝，allow_depleted_fallback 打开后才允许兜底。
Route explain 能说明 group/rate/pricing/balance/fallback。
请求日志能追溯经济上下文，但不存 prompt/response/secret。
采集中心能调试任务、session、endpoint、facts、redacted raw。
变更中心覆盖 group/rate/price/model/key/collector/route/balance 事件。
```

P9 fails if any of these are true:

```text
NewAPI 仍然只是 UI 类型，没有真实 adapter。
Sub2API 和 NewAPI 逻辑继续塞在同一个巨大探针函数里。
UI 主要业务展示仍然从 collector_snapshots.normalized_json 解析倍率。
station_group_bindings 无法区分 station-level group 和 key-level binding。
没有 group_key_hash，缺 group id 的站点无法稳定去重。
full 采集没有 parent/child run 关系。
group_rate_only 被当作完整价格参与 cheap_first。
余额耗尽候选在默认策略下仍然被选中。
新增 token/cookie/session 出现在 raw snapshot、run error、request log 或 UI list API 中。
价格 / 倍率页面退回 pricing_rules CRUD 表。
采集中心泄露完整 token、cookie、password 或 Authorization header。
```

---

## Self-Review

### Spec Coverage

- P9 goals and non-goals: covered in Product Boundary and Completion Gates.
- Backend module structure: Tasks 6, 7, 8, 9, 10, 11, 12, 13.
- Data model upgrades: Tasks 2, 3, 4, 5, 14.
- `station_group_bindings`: Tasks 2, 3, 4, 5, 10, 18, 19.
- `group_rate_records`: Tasks 2, 3, 4, 10, 20.
- `collector_runs`: Tasks 2, 3, 4, 7, 17.
- `station_credentials` token/session expansion: Task 9.
- `pricing_rules` extension and `group_rate_only`: Tasks 2, 14, 20.
- `station_keys` group binding/rate/balance scope: Tasks 2, 5, 19.
- URL helper and `/v1` normalization: Task 6 and Task 8.
- Sub2API adapter: Tasks 8, 9, 10.
- NewAPI adapter: Task 11.
- OpenAI-compatible adapter: Task 12.
- Routing/request-log economics: Task 14 and Task 21.
- Change center events: Task 15 and Task 22.
- UI responsibilities: Tasks 17 through 23.
- Scheduling/settings: Task 23.
- Security/redaction: Tasks 7, 9, 25 and safety scan expansion in Task 5.
- Migration strategy: Task 5.
- Testing and verification: every task has narrow checks; Task 25 has full verification.

### Placeholder Scan

The plan avoids placeholder markers and vague catch-all work items. When an implementation step depends on existing project helper names, the plan gives the exact target behavior, exact file, and exact fallback instruction. Every testing step names concrete tests or commands.

### Type Consistency

- Backend `StationGroupBinding` maps to frontend `StationGroupBinding`.
- Backend `GroupRateRecord` maps to frontend `GroupRateRecord`.
- Backend `CollectorRun` maps to frontend `CollectorRun`.
- `groupKeyHash`, `groupIdHash`, `bindingKind`, `bindingStatus`, `parentRunId`, `normalizationStatus`, and `allowDepletedFallback` are consistently camelCase in TypeScript and snake_case in Rust/SQLite.
- Collector task values are consistently `detect`, `balance`, `groups`, `models`, `full`.
- Run statuses are consistently `running`, `success`, `partial`, `failed`, `manual_required`.
