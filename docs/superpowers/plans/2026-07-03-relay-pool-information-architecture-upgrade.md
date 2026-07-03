# Relay Pool Information Architecture Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade Relay Pool Desktop from CRUD-style relay configuration screens into a local AI relay asset management and routing gateway console with persistent change tracking, asset-focused station views, and cross-station price/rate comparison.

**Architecture:** Add a persistent `change_events` domain first, then make navigation and pages consume it. Keep Station, Station Key, Pricing Rule, Balance Snapshot, Request Log, and Collector Snapshot as existing source objects; add small focused view-model helpers on the frontend so pages answer product questions instead of exposing database tables directly. Use high-density tables and right-side drawers/inspectors instead of card-heavy layouts.

**Tech Stack:** Tauri 2, React, TypeScript, Vite, Tailwind CSS, SQLite through Rust `rusqlite`, existing command/API wrappers, existing UI primitives in `src/components/ui`, lucide-react icons.

---

## Confirmed Product Decisions

- Rename `工作台` to `总览`.
- Rename `中转站` to `中转站资产`.
- Rename `价格表` to `价格 / 倍率`.
- Add `变更中心` as a first-class page.
- Remove `信息采集` from first-level navigation after its core affordances are reachable from station asset details.
- Keep `Key 池`, `路由规则`, `渠道状态`, `请求日志`, and `设置`.
- Build `变更中心` with persistent `change_events`, not a temporary derived-only page.
- Sidebar badge counts unread `critical` and `warning` events only; `info` events appear in the page but do not create badge pressure.
- First station asset detail interaction uses a right-side drawer so the main table stays large.
- Do not create a duplicate station page. Upgrade the existing stations page.
- Do not keep pricing as a raw `pricing_rules` CRUD/table page. Upgrade it to cross-station comparison.

## Execution Rules

- Start with `git status --short -- .` and `git diff --cached --name-only`; preserve user changes.
- Do not use `git add .`, `git add -A`, or `git commit -a`.
- Stage exact files only.
- Keep commits by concern:
  - data model/API
  - change center UI
  - navigation shell
  - station assets UI
  - price/rate matrix UI
  - docs/final polish
- Run frontend checks after every frontend batch: `pnpm.cmd tsc --noEmit` and `pnpm.cmd build`.
- Run Rust checks after every Rust/schema batch: `cargo check --manifest-path .\src-tauri\Cargo.toml`.
- When a task modifies tests, run the narrow test first, then the broader check.

## File Structure Map

### Backend domain and persistence

- Modify: `src-tauri/src/services/database.rs`
  - Add `change_events` schema migration.
  - Add row mapping, list, insert/upsert, mark read, dismiss, resolve helpers.
  - Add event generation call sites near existing balance, pricing, collector, and health persistence helpers.
- Modify: `src-tauri/src/commands/mod.rs`
  - Expose Tauri commands for listing and mutating change events.
- Modify: `src-tauri/src/lib.rs`
  - Register new Tauri commands.
- Create: `src-tauri/src/models/change_events.rs`
  - Define Rust data structs and enum-like string fields for `ChangeEvent`.
- Modify: `src-tauri/src/models/mod.rs`
  - Export `change_events`.
- Create: `src-tauri/src/services/change_events.rs`
  - Keep event type constants, severity helpers, dedupe key builders, and JSON value helpers out of `database.rs`.

### Frontend API and types

- Create: `src/lib/types/changeEvents.ts`
  - TypeScript types matching Rust `ChangeEvent`.
- Create: `src/lib/api/changeEvents.ts`
  - Tauri invoke wrappers plus browser fallback.
- Create: `src/lib/mock/changeEvents.ts`
  - Development/mock data used when Tauri invoke is unavailable.
- Modify: `src/lib/mock/index.ts`
  - Export change event memory store if the mock index is used by existing APIs.

### Navigation and shell

- Modify: `src/lib/types/navigation.ts`
  - Add route id `changes`.
  - Remove or hide route id `collectors` from first-level routes while keeping page id if still needed internally.
- Modify: `src/app/routes.tsx`
  - Rename labels/descriptions and add `变更中心`.
- Modify: `src/app/App.tsx`
  - Route `changes` to `ChangeCenterPage`.
  - Keep `CollectorsPage` reachable only if a drawer action or internal page still uses it.
- Modify: `src/components/shell/AppShell.tsx`
  - Load unread critical/warning count and show a compact badge on the `变更中心` route icon.

### Change center UI

- Create: `src/features/changes/ChangeCenterPage.tsx`
  - High-density event list, filters, right-side inspector, status actions.
- Create: `src/features/changes/changeEventViewModels.ts`
  - Severity labels, event type labels, object labels, filtering helpers.

### Station assets UI

- Modify: `src/features/stations/StationsPage.tsx`
  - Rename page to `中转站资产`.
  - Replace row stack with high-density asset table.
  - Replace details dialog with right-side drawer.
  - Move collection status/history affordances into drawer tabs.
- Modify: `src/features/stations/components/StationDetailPanel.tsx`
  - Reuse or refactor into drawer content if current component fits.
- Create: `src/features/stations/stationAssetViewModels.ts`
  - Build station asset rows by joining `Station`, `StationKey`, `CollectorSnapshot`, `BalanceSnapshot`, and `ChangeEvent`.

### Price/rate comparison UI

- Modify: `src/features/pricing/PricingPage.tsx`
  - Rename to `价格 / 倍率`.
  - Replace raw pricing table as primary view with matrix tabs.
- Create: `src/features/pricing/pricingMatrix.ts`
  - Build model price matrix, group rate matrix, model availability matrix.
- Create: `src/features/pricing/rateSnapshotParser.ts`
  - Parse normalized collector snapshot groups/rates into stable view rows.

### Dashboard and cross-page links

- Modify: `src/features/dashboard/DashboardPage.tsx`
  - Rename title to `总览`.
  - Replace generic activity feed with risk-first summary from `change_events`.
  - Keep proxy entry, failure rate, cost, key availability.
- Modify: `src/features/logs/LogsPage.tsx`
  - Preserve log inspector; add navigation affordance from linked change events only if route plumbing is already simple.
- Modify: `src/features/channels/ChannelStatusPage.tsx`
  - Keep channel health UI; optionally add linked unresolved event count per channel/key.
- Modify: `src/features/routing/RoutingPage.tsx`
  - Keep routing simulator; optionally show route-impacted events after data model exists.

### Documentation

- Modify: `docs/PROJECT_PLAN.md`
  - Update product definition wording and page responsibilities.
- Create or modify: `docs/superpowers/plans/2026-07-03-relay-pool-information-architecture-upgrade.md`
  - Keep this plan updated if implementation discoveries require changes.

---

## Task 1: Baseline Audit and Worktree Safety

**Files:**
- Read-only: repository status and key files

- [ ] **Step 1: Inspect current working tree**

Run:

```powershell
git status --short -- .
git diff --cached --name-only
```

Expected:

- Existing user or prior work may be present.
- Do not revert it.
- Record any modified files that overlap this plan.

- [ ] **Step 2: Inspect current route/page wiring**

Run:

```powershell
Get-Content -LiteralPath 'src\app\routes.tsx' -Encoding utf8
Get-Content -LiteralPath 'src\app\App.tsx' -Encoding utf8
Get-Content -LiteralPath 'src\lib\types\navigation.ts' -Encoding utf8
```

Expected:

- Current pages include dashboard, stations, keyPool, channels, collectors, pricing, routing, logs, settings.
- Keep notes on exact route id names before editing.

- [ ] **Step 3: Inspect database command registration**

Run:

```powershell
Get-Content -LiteralPath 'src-tauri\src\commands\mod.rs' -Encoding utf8
Get-Content -LiteralPath 'src-tauri\src\lib.rs' -Encoding utf8
Get-Content -LiteralPath 'src-tauri\src\models\mod.rs' -Encoding utf8
```

Expected:

- Existing command style uses `#[tauri::command]` wrappers around `AppDatabase`.
- Existing models are exported from `src-tauri/src/models/mod.rs`.

- [ ] **Step 4: Confirm no edits were made**

Run:

```powershell
git status --short -- .
```

Expected:

- Status unchanged from Step 1.

---

## Task 2: Add Change Event Types and SQLite Schema

**Files:**
- Create: `src-tauri/src/models/change_events.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/services/change_events.rs`
- Modify: `src-tauri/src/services/database.rs`
- Test: `src-tauri/src/services/database.rs` unit tests module

- [ ] **Step 1: Add Rust model file**

Create `src-tauri/src/models/change_events.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ChangeEvent {
    pub id: String,
    pub severity: String,
    pub event_type: String,
    pub status: String,
    pub title: String,
    pub message: String,
    pub object_type: String,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub pricing_rule_id: Option<String>,
    pub request_log_id: Option<String>,
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub impact_json: Option<String>,
    pub dedupe_key: String,
    pub source: String,
    pub detected_at: String,
    pub resolved_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertChangeEventInput {
    pub severity: String,
    pub event_type: String,
    pub title: String,
    pub message: String,
    pub object_type: String,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub pricing_rule_id: Option<String>,
    pub request_log_id: Option<String>,
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub impact_json: Option<String>,
    pub dedupe_key: String,
    pub source: String,
}
```

- [ ] **Step 2: Export the model**

Modify `src-tauri/src/models/mod.rs` and add:

```rust
pub mod change_events;
```

- [ ] **Step 3: Add change event helper module**

Create `src-tauri/src/services/change_events.rs`:

```rust
use serde_json::json;

pub const SEVERITY_CRITICAL: &str = "critical";
pub const SEVERITY_WARNING: &str = "warning";
pub const SEVERITY_INFO: &str = "info";

pub const STATUS_UNREAD: &str = "unread";
pub const STATUS_READ: &str = "read";
pub const STATUS_DISMISSED: &str = "dismissed";
pub const STATUS_RESOLVED: &str = "resolved";

pub fn balance_dedupe_key(station_id: &str, status: &str) -> String {
    format!("balance:{status}:station:{station_id}")
}

pub fn station_health_dedupe_key(station_id: &str, event_type: &str) -> String {
    format!("{event_type}:station:{station_id}")
}

pub fn station_key_dedupe_key(station_key_id: &str, event_type: &str) -> String {
    format!("{event_type}:station_key:{station_key_id}")
}

pub fn collector_dedupe_key(station_id: &str, event_type: &str) -> String {
    format!("{event_type}:collector:{station_id}")
}

pub fn pricing_dedupe_key(station_id: &str, group_name: Option<&str>, model: &str) -> String {
    format!(
        "price_changed:station:{station_id}:group:{}:model:{model}",
        group_name.unwrap_or("-")
    )
}

pub fn rate_dedupe_key(station_id: &str, group_name: &str) -> String {
    format!("rate_changed:station:{station_id}:group:{group_name}")
}

pub fn model_dedupe_key(station_id: &str, event_type: &str, model: &str) -> String {
    format!("{event_type}:station:{station_id}:model:{model}")
}

pub fn value_change_json<T: serde::Serialize>(old_value: T, new_value: T) -> String {
    json!({
        "old": old_value,
        "new": new_value
    })
    .to_string()
}
```

- [ ] **Step 4: Register service module**

Modify `src-tauri/src/services/mod.rs` and add:

```rust
pub mod change_events;
```

- [ ] **Step 5: Add schema migration**

Modify `initialize_schema` or the existing schema initializer in `src-tauri/src/services/database.rs` by adding:

```rust
connection.execute_batch(
    r#"
    CREATE TABLE IF NOT EXISTS change_events (
        id TEXT PRIMARY KEY,
        severity TEXT NOT NULL,
        event_type TEXT NOT NULL,
        status TEXT NOT NULL,
        title TEXT NOT NULL,
        message TEXT NOT NULL,
        object_type TEXT NOT NULL,
        object_id TEXT,
        station_id TEXT,
        station_key_id TEXT,
        pricing_rule_id TEXT,
        request_log_id TEXT,
        old_value_json TEXT,
        new_value_json TEXT,
        impact_json TEXT,
        dedupe_key TEXT NOT NULL UNIQUE,
        source TEXT NOT NULL,
        detected_at TEXT NOT NULL,
        resolved_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_change_events_status_severity_updated
        ON change_events(status, severity, updated_at DESC);

    CREATE INDEX IF NOT EXISTS idx_change_events_station_updated
        ON change_events(station_id, updated_at DESC);

    CREATE INDEX IF NOT EXISTS idx_change_events_station_key_updated
        ON change_events(station_key_id, updated_at DESC);
    "#,
)
.map_err(|error| format!("初始化变更中心 schema 失败: {error}"))?;
```

- [ ] **Step 6: Add database tests for schema existence**

In the existing `#[cfg(test)]` module in `src-tauri/src/services/database.rs`, add:

```rust
#[test]
fn change_events_table_is_initialized() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let connection = database.connection().expect("connection");
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'change_events'",
            [],
            |row| row.get(0),
        )
        .expect("table count");
    assert_eq!(count, 1);
}
```

- [ ] **Step 7: Run narrow Rust test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml change_events_table_is_initialized --lib
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

- [ ] **Step 9: Commit data schema**

Run exact-path staging only:

```powershell
git add -- src-tauri/src/models/change_events.rs src-tauri/src/models/mod.rs src-tauri/src/services/change_events.rs src-tauri/src/services/mod.rs src-tauri/src/services/database.rs
git commit -m "feat: add persistent change event schema"
```

Expected:

- Commit succeeds.
- If unrelated user changes exist in any file, inspect diff and stage only the hunks for this task.

---

## Task 3: Add Change Event Database Operations and Commands

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add imports in database**

In `src-tauri/src/services/database.rs`, add imports near existing model imports:

```rust
use crate::models::change_events::{ChangeEvent, UpsertChangeEventInput};
use crate::services::change_events::{STATUS_DISMISSED, STATUS_READ, STATUS_RESOLVED, STATUS_UNREAD};
```

- [ ] **Step 2: Add public AppDatabase methods**

Inside `impl AppDatabase`, add:

```rust
pub fn list_change_events(&self) -> Result<Vec<ChangeEvent>, String> {
    let connection = self.connection()?;
    list_change_events_from_connection(&connection)
}

pub fn upsert_change_event(&self, input: UpsertChangeEventInput) -> Result<ChangeEvent, String> {
    let connection = self.connection()?;
    upsert_change_event_in_connection(&connection, input)
}

pub fn mark_change_event_read(&self, id: String) -> Result<ChangeEvent, String> {
    let connection = self.connection()?;
    update_change_event_status_in_connection(&connection, &id, STATUS_READ)
}

pub fn dismiss_change_event(&self, id: String) -> Result<ChangeEvent, String> {
    let connection = self.connection()?;
    update_change_event_status_in_connection(&connection, &id, STATUS_DISMISSED)
}

pub fn resolve_change_event(&self, id: String) -> Result<ChangeEvent, String> {
    let connection = self.connection()?;
    resolve_change_event_in_connection(&connection, &id)
}
```

- [ ] **Step 3: Add row mapper and SQL helpers**

Add helper functions near similar row mappers:

```rust
fn row_to_change_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChangeEvent> {
    Ok(ChangeEvent {
        id: row.get("id")?,
        severity: row.get("severity")?,
        event_type: row.get("event_type")?,
        status: row.get("status")?,
        title: row.get("title")?,
        message: row.get("message")?,
        object_type: row.get("object_type")?,
        object_id: row.get("object_id")?,
        station_id: row.get("station_id")?,
        station_key_id: row.get("station_key_id")?,
        pricing_rule_id: row.get("pricing_rule_id")?,
        request_log_id: row.get("request_log_id")?,
        old_value_json: row.get("old_value_json")?,
        new_value_json: row.get("new_value_json")?,
        impact_json: row.get("impact_json")?,
        dedupe_key: row.get("dedupe_key")?,
        source: row.get("source")?,
        detected_at: row.get("detected_at")?,
        resolved_at: row.get("resolved_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

fn list_change_events_from_connection(connection: &Connection) -> Result<Vec<ChangeEvent>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, severity, event_type, status, title, message, object_type, object_id,
                    station_id, station_key_id, pricing_rule_id, request_log_id,
                    old_value_json, new_value_json, impact_json, dedupe_key, source,
                    detected_at, resolved_at, created_at, updated_at
             FROM change_events
             ORDER BY updated_at DESC, detected_at DESC",
        )
        .map_err(|error| format!("准备读取变更事件失败: {error}"))?;
    statement
        .query_map([], row_to_change_event)
        .map_err(|error| format!("读取变更事件失败: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("解析变更事件失败: {error}"))
}

fn upsert_change_event_in_connection(
    connection: &Connection,
    input: UpsertChangeEventInput,
) -> Result<ChangeEvent, String> {
    let now = now_string();
    let id = next_id("change");
    connection
        .execute(
            "INSERT INTO change_events (
                id, severity, event_type, status, title, message, object_type, object_id,
                station_id, station_key_id, pricing_rule_id, request_log_id,
                old_value_json, new_value_json, impact_json, dedupe_key, source,
                detected_at, resolved_at, created_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, NULL, ?19, ?20)
             ON CONFLICT(dedupe_key) DO UPDATE SET
                severity = excluded.severity,
                event_type = excluded.event_type,
                status = CASE
                    WHEN change_events.status = 'dismissed' THEN change_events.status
                    ELSE 'unread'
                END,
                title = excluded.title,
                message = excluded.message,
                object_type = excluded.object_type,
                object_id = excluded.object_id,
                station_id = excluded.station_id,
                station_key_id = excluded.station_key_id,
                pricing_rule_id = excluded.pricing_rule_id,
                request_log_id = excluded.request_log_id,
                old_value_json = excluded.old_value_json,
                new_value_json = excluded.new_value_json,
                impact_json = excluded.impact_json,
                source = excluded.source,
                detected_at = excluded.detected_at,
                resolved_at = NULL,
                updated_at = excluded.updated_at",
            rusqlite::params![
                id,
                input.severity,
                input.event_type,
                STATUS_UNREAD,
                input.title,
                input.message,
                input.object_type,
                input.object_id,
                input.station_id,
                input.station_key_id,
                input.pricing_rule_id,
                input.request_log_id,
                input.old_value_json,
                input.new_value_json,
                input.impact_json,
                input.dedupe_key,
                input.source,
                now,
                now,
                now
            ],
        )
        .map_err(|error| format!("写入变更事件失败: {error}"))?;
    change_event_by_dedupe_key(connection, &input.dedupe_key)
}

fn change_event_by_dedupe_key(connection: &Connection, dedupe_key: &str) -> Result<ChangeEvent, String> {
    connection
        .query_row(
            "SELECT id, severity, event_type, status, title, message, object_type, object_id,
                    station_id, station_key_id, pricing_rule_id, request_log_id,
                    old_value_json, new_value_json, impact_json, dedupe_key, source,
                    detected_at, resolved_at, created_at, updated_at
             FROM change_events
             WHERE dedupe_key = ?1",
            rusqlite::params![dedupe_key],
            row_to_change_event,
        )
        .map_err(|error| format!("读取变更事件失败: {error}"))
}

fn change_event_by_id(connection: &Connection, id: &str) -> Result<ChangeEvent, String> {
    connection
        .query_row(
            "SELECT id, severity, event_type, status, title, message, object_type, object_id,
                    station_id, station_key_id, pricing_rule_id, request_log_id,
                    old_value_json, new_value_json, impact_json, dedupe_key, source,
                    detected_at, resolved_at, created_at, updated_at
             FROM change_events
             WHERE id = ?1",
            rusqlite::params![id],
            row_to_change_event,
        )
        .map_err(|error| format!("读取变更事件失败: {error}"))
}

fn update_change_event_status_in_connection(
    connection: &Connection,
    id: &str,
    status: &str,
) -> Result<ChangeEvent, String> {
    let now = now_string();
    connection
        .execute(
            "UPDATE change_events SET status = ?2, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id, status, now],
        )
        .map_err(|error| format!("更新变更事件状态失败: {error}"))?;
    change_event_by_id(connection, id)
}

fn resolve_change_event_in_connection(connection: &Connection, id: &str) -> Result<ChangeEvent, String> {
    let now = now_string();
    connection
        .execute(
            "UPDATE change_events SET status = ?2, resolved_at = ?3, updated_at = ?3 WHERE id = ?1",
            rusqlite::params![id, STATUS_RESOLVED, now],
        )
        .map_err(|error| format!("解决变更事件失败: {error}"))?;
    change_event_by_id(connection, id)
}
```

If `now_string()` or `next_id()` names differ in the current file, use the existing timestamp/id helpers already used for `pricing_rules`, `balance_snapshots`, and `model_aliases`.

- [ ] **Step 4: Add database round-trip test**

Add:

```rust
#[test]
fn change_event_upsert_dedupes_and_can_be_resolved() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let first = database
        .upsert_change_event(UpsertChangeEventInput {
            severity: "warning".to_string(),
            event_type: "balance_low".to_string(),
            title: "余额偏低".to_string(),
            message: "测试站点余额低于阈值".to_string(),
            object_type: "station".to_string(),
            object_id: Some("station-1".to_string()),
            station_id: Some("station-1".to_string()),
            station_key_id: None,
            pricing_rule_id: None,
            request_log_id: None,
            old_value_json: None,
            new_value_json: Some("{\"value\":4.2}".to_string()),
            impact_json: None,
            dedupe_key: "balance:low:station:station-1".to_string(),
            source: "balance".to_string(),
        })
        .expect("first event");
    let second = database
        .upsert_change_event(UpsertChangeEventInput {
            severity: "warning".to_string(),
            event_type: "balance_low".to_string(),
            title: "余额偏低".to_string(),
            message: "测试站点余额仍低于阈值".to_string(),
            object_type: "station".to_string(),
            object_id: Some("station-1".to_string()),
            station_id: Some("station-1".to_string()),
            station_key_id: None,
            pricing_rule_id: None,
            request_log_id: None,
            old_value_json: None,
            new_value_json: Some("{\"value\":3.1}".to_string()),
            impact_json: None,
            dedupe_key: "balance:low:station:station-1".to_string(),
            source: "balance".to_string(),
        })
        .expect("second event");
    assert_eq!(first.id, second.id);
    assert_eq!(second.status, "unread");
    assert!(second.message.contains("仍低于"));

    let resolved = database
        .resolve_change_event(second.id.clone())
        .expect("resolved event");
    assert_eq!(resolved.status, "resolved");
    assert!(resolved.resolved_at.is_some());

    let events = database.list_change_events().expect("events");
    assert_eq!(events.len(), 1);
}
```

- [ ] **Step 5: Run narrow test**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml change_event_upsert_dedupes_and_can_be_resolved --lib
```

Expected:

- PASS.

- [ ] **Step 6: Add commands**

Modify `src-tauri/src/commands/mod.rs`:

```rust
use crate::models::change_events::{ChangeEvent, UpsertChangeEventInput};
```

Add command functions:

```rust
#[tauri::command]
pub fn list_change_events(database: State<'_, AppDatabase>) -> Result<Vec<ChangeEvent>, String> {
    database.list_change_events()
}

#[tauri::command]
pub fn upsert_change_event(
    database: State<'_, AppDatabase>,
    input: UpsertChangeEventInput,
) -> Result<ChangeEvent, String> {
    database.upsert_change_event(input)
}

#[tauri::command]
pub fn mark_change_event_read(
    database: State<'_, AppDatabase>,
    id: String,
) -> Result<ChangeEvent, String> {
    database.mark_change_event_read(id)
}

#[tauri::command]
pub fn dismiss_change_event(
    database: State<'_, AppDatabase>,
    id: String,
) -> Result<ChangeEvent, String> {
    database.dismiss_change_event(id)
}

#[tauri::command]
pub fn resolve_change_event(
    database: State<'_, AppDatabase>,
    id: String,
) -> Result<ChangeEvent, String> {
    database.resolve_change_event(id)
}
```

- [ ] **Step 7: Register commands**

Modify `src-tauri/src/lib.rs` command handler list and add:

```rust
commands::list_change_events,
commands::upsert_change_event,
commands::mark_change_event_read,
commands::dismiss_change_event,
commands::resolve_change_event,
```

- [ ] **Step 8: Run Rust checks**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml change_event --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 9: Commit database operations**

Run exact-path staging:

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: expose change event operations"
```

Expected:

- Commit succeeds.

---

## Task 4: Generate High-Value Change Events from Existing Backend Flows

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/change_events.rs`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add event builder helpers**

Extend `src-tauri/src/services/change_events.rs`:

```rust
use crate::models::change_events::UpsertChangeEventInput;

pub fn station_balance_event(
    station_id: &str,
    status: &str,
    value: Option<f64>,
    threshold: Option<f64>,
) -> Option<UpsertChangeEventInput> {
    match status {
        "depleted" => Some(UpsertChangeEventInput {
            severity: SEVERITY_CRITICAL.to_string(),
            event_type: "balance_depleted".to_string(),
            title: "余额耗尽".to_string(),
            message: format!("站点余额已耗尽，当前余额 {}", value.map(|v| v.to_string()).unwrap_or_else(|| "未知".to_string())),
            object_type: "station".to_string(),
            object_id: Some(station_id.to_string()),
            station_id: Some(station_id.to_string()),
            station_key_id: None,
            pricing_rule_id: None,
            request_log_id: None,
            old_value_json: None,
            new_value_json: Some(value_change_json(serde_json::Value::Null, json!({ "value": value, "threshold": threshold }))),
            impact_json: Some(json!({ "routingRisk": "deprioritize_or_block" }).to_string()),
            dedupe_key: balance_dedupe_key(station_id, "depleted"),
            source: "balance".to_string(),
        }),
        "low" => Some(UpsertChangeEventInput {
            severity: SEVERITY_WARNING.to_string(),
            event_type: "balance_low".to_string(),
            title: "余额偏低".to_string(),
            message: format!("站点余额低于阈值，当前余额 {}", value.map(|v| v.to_string()).unwrap_or_else(|| "未知".to_string())),
            object_type: "station".to_string(),
            object_id: Some(station_id.to_string()),
            station_id: Some(station_id.to_string()),
            station_key_id: None,
            pricing_rule_id: None,
            request_log_id: None,
            old_value_json: None,
            new_value_json: Some(json!({ "value": value, "threshold": threshold }).to_string()),
            impact_json: Some(json!({ "routingRisk": "deprioritize" }).to_string()),
            dedupe_key: balance_dedupe_key(station_id, "low"),
            source: "balance".to_string(),
        }),
        _ => None,
    }
}

pub fn key_health_event(
    station_key_id: &str,
    station_id: &str,
    consecutive_failures: i64,
    last_error: Option<&str>,
    cooldown_until: Option<&str>,
) -> Option<UpsertChangeEventInput> {
    if consecutive_failures <= 0 && cooldown_until.is_none() {
        return None;
    }
    Some(UpsertChangeEventInput {
        severity: if consecutive_failures >= 3 { SEVERITY_CRITICAL } else { SEVERITY_WARNING }.to_string(),
        event_type: "key_invalid".to_string(),
        title: "Key 健康异常".to_string(),
        message: format!("Key 连续失败 {consecutive_failures} 次{}", last_error.map(|v| format!("：{v}")).unwrap_or_default()),
        object_type: "station_key".to_string(),
        object_id: Some(station_key_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: Some(station_key_id.to_string()),
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json: Some(json!({ "consecutiveFailures": consecutive_failures, "cooldownUntil": cooldown_until }).to_string()),
        impact_json: Some(json!({ "routingRisk": "candidate_filtered_or_deprioritized" }).to_string()),
        dedupe_key: station_key_dedupe_key(station_key_id, "key_invalid"),
        source: "health".to_string(),
    })
}

pub fn collector_failed_event(station_id: &str, error_message: Option<&str>) -> UpsertChangeEventInput {
    UpsertChangeEventInput {
        severity: SEVERITY_WARNING.to_string(),
        event_type: "collector_failed".to_string(),
        title: "站点采集失败".to_string(),
        message: error_message.unwrap_or("采集失败，未返回详细错误").to_string(),
        object_type: "station".to_string(),
        object_id: Some(station_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json: None,
        impact_json: Some(json!({ "staleDataRisk": true }).to_string()),
        dedupe_key: collector_dedupe_key(station_id, "collector_failed"),
        source: "collector".to_string(),
    }
}
```

If `serde_json::json` is already imported once in this module, keep a single import.

- [ ] **Step 2: Emit balance events when balance snapshots are inserted**

In the existing `upsert_balance_snapshot` database helper, after inserting the snapshot and before returning the saved row, call:

```rust
if input.scope == "station" {
    if let Some(event) = crate::services::change_events::station_balance_event(
        &input.station_id,
        &input.status,
        input.value,
        input.low_balance_threshold,
    ) {
        let _ = upsert_change_event_in_connection(connection, event);
    }
}
```

Use exact field names from the current `UpsertBalanceSnapshotInput`. If the helper currently receives a different local variable name than `input`, adapt only that variable name.

- [ ] **Step 3: Emit collector failure events**

In the helper that inserts `collector_snapshots`, after a failed snapshot is saved, add:

```rust
if snapshot.status == "failed" {
    let event = crate::services::change_events::collector_failed_event(
        &snapshot.station_id,
        snapshot.error_message.as_deref(),
    );
    let _ = upsert_change_event_in_connection(connection, event);
}
```

Use the local saved snapshot variable name in the current function.

- [ ] **Step 4: Emit key health events**

In the health update helpers that write `station_key_health`, after the updated health row is available, add:

```rust
if let Some(station_id) = station_id_for_key(connection, &health.station_key_id)? {
    if let Some(event) = crate::services::change_events::key_health_event(
        &health.station_key_id,
        &station_id,
        health.consecutive_failures,
        health.last_error_summary.as_deref(),
        health.cooldown_until.as_deref(),
    ) {
        let _ = upsert_change_event_in_connection(connection, event);
    }
}
```

Add helper:

```rust
fn station_id_for_key(connection: &Connection, station_key_id: &str) -> Result<Option<String>, String> {
    let result = connection
        .query_row(
            "SELECT station_id FROM station_keys WHERE id = ?1",
            rusqlite::params![station_key_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("读取 Key 所属站点失败: {error}"))?;
    Ok(result)
}
```

Ensure `rusqlite::OptionalExtension` is imported if not already imported.

- [ ] **Step 5: Add event generation tests**

Add one focused test for balance:

```rust
#[test]
fn low_balance_snapshot_creates_change_event() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = database
        .create_station(CreateStationInput {
            name: "Low Balance Relay".to_string(),
            station_type: "sub2api".to_string(),
            base_url: "https://low.example/v1".to_string(),
            api_key: "sk-test".to_string(),
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: Some(10.0),
            note: None,
        })
        .expect("station");
    database
        .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
            station_id: station.id.clone(),
            station_key_id: None,
            scope: "station".to_string(),
            value: Some(4.0),
            currency: "CNY".to_string(),
            credit_unit: None,
            used_value: None,
            total_value: None,
            low_balance_threshold: Some(10.0),
            status: "low".to_string(),
            source: "test".to_string(),
            confidence: 1.0,
            collected_at: None,
        })
        .expect("balance");
    let events = database.list_change_events().expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "balance_low");
    assert_eq!(events[0].severity, "warning");
    assert_eq!(events[0].station_id.as_deref(), Some(station.id.as_str()));
}
```

If current `CreateStationInput` or `UpsertBalanceSnapshotInput` field names differ, inspect the existing input types and adjust this test to compile against the current definitions without changing test intent.

- [ ] **Step 6: Run tests and checks**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml low_balance_snapshot_creates_change_event --lib
cargo test --manifest-path .\src-tauri\Cargo.toml change_event --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 7: Commit high-value event generation**

Run:

```powershell
git add -- src-tauri/src/services/change_events.rs src-tauri/src/services/database.rs
git commit -m "feat: generate core change events"
```

Expected:

- Commit succeeds.

---

## Task 5: Add Frontend Change Event API, Types, and Mock Fallback

**Files:**
- Create: `src/lib/types/changeEvents.ts`
- Create: `src/lib/mock/changeEvents.ts`
- Create: `src/lib/api/changeEvents.ts`
- Modify: `src/lib/mock/index.ts` if needed by project exports

- [ ] **Step 1: Add frontend types**

Create `src/lib/types/changeEvents.ts`:

```ts
export type ChangeSeverity = "critical" | "warning" | "info";
export type ChangeEventStatus = "unread" | "read" | "dismissed" | "resolved";

export type ChangeObjectType =
  | "station"
  | "station_key"
  | "pricing_rule"
  | "routing_rule"
  | "request_log"
  | "channel"
  | "collector";

export type ChangeEvent = {
  id: string;
  severity: ChangeSeverity;
  eventType: string;
  status: ChangeEventStatus;
  title: string;
  message: string;
  objectType: ChangeObjectType | string;
  objectId: string | null;
  stationId: string | null;
  stationKeyId: string | null;
  pricingRuleId: string | null;
  requestLogId: string | null;
  oldValueJson: string | null;
  newValueJson: string | null;
  impactJson: string | null;
  dedupeKey: string;
  source: string;
  detectedAt: string;
  resolvedAt: string | null;
  createdAt: string;
  updatedAt: string;
};

export type UpsertChangeEventInput = {
  severity: ChangeSeverity;
  eventType: string;
  title: string;
  message: string;
  objectType: ChangeObjectType | string;
  objectId: string | null;
  stationId: string | null;
  stationKeyId: string | null;
  pricingRuleId: string | null;
  requestLogId: string | null;
  oldValueJson: string | null;
  newValueJson: string | null;
  impactJson: string | null;
  dedupeKey: string;
  source: string;
};
```

- [ ] **Step 2: Add mock data**

Create `src/lib/mock/changeEvents.ts`:

```ts
import type { ChangeEvent, UpsertChangeEventInput } from "@/lib/types/changeEvents";

let memoryChangeEvents: ChangeEvent[] = [
  {
    id: "change-demo-balance-low",
    severity: "warning",
    eventType: "balance_low",
    status: "unread",
    title: "余额偏低",
    message: "Orchid Relay 余额低于阈值，可能影响 cheap_first 路由。",
    objectType: "station",
    objectId: "station-orchid",
    stationId: "station-orchid",
    stationKeyId: null,
    pricingRuleId: null,
    requestLogId: null,
    oldValueJson: null,
    newValueJson: JSON.stringify({ value: 4.2, threshold: 10 }),
    impactJson: JSON.stringify({ routingRisk: "deprioritize" }),
    dedupeKey: "balance:low:station:station-orchid",
    source: "balance",
    detectedAt: new Date(Date.now() - 1000 * 60 * 20).toISOString(),
    resolvedAt: null,
    createdAt: new Date(Date.now() - 1000 * 60 * 20).toISOString(),
    updatedAt: new Date(Date.now() - 1000 * 60 * 20).toISOString(),
  },
  {
    id: "change-demo-model-added",
    severity: "info",
    eventType: "model_added",
    status: "unread",
    title: "模型新增",
    message: "Blue Pool 新增模型 gpt-5-mini。",
    objectType: "pricing_rule",
    objectId: "pricing-demo",
    stationId: "station-blue",
    stationKeyId: null,
    pricingRuleId: "pricing-demo",
    requestLogId: null,
    oldValueJson: null,
    newValueJson: JSON.stringify({ model: "gpt-5-mini" }),
    impactJson: null,
    dedupeKey: "model_added:station:station-blue:model:gpt-5-mini",
    source: "collector",
    detectedAt: new Date(Date.now() - 1000 * 60 * 90).toISOString(),
    resolvedAt: null,
    createdAt: new Date(Date.now() - 1000 * 60 * 90).toISOString(),
    updatedAt: new Date(Date.now() - 1000 * 60 * 90).toISOString(),
  },
];

export function listMockChangeEvents() {
  return Promise.resolve([...memoryChangeEvents]);
}

export function upsertMockChangeEvent(input: UpsertChangeEventInput) {
  const now = new Date().toISOString();
  const existingIndex = memoryChangeEvents.findIndex((event) => event.dedupeKey === input.dedupeKey);
  const next: ChangeEvent = {
    id: existingIndex >= 0 ? memoryChangeEvents[existingIndex].id : `change-${Date.now()}`,
    status: "unread",
    detectedAt: now,
    createdAt: existingIndex >= 0 ? memoryChangeEvents[existingIndex].createdAt : now,
    updatedAt: now,
    resolvedAt: null,
    ...input,
  };
  if (existingIndex >= 0) {
    memoryChangeEvents = memoryChangeEvents.map((event, index) => (index === existingIndex ? next : event));
  } else {
    memoryChangeEvents = [next, ...memoryChangeEvents];
  }
  return Promise.resolve(next);
}

export function updateMockChangeEventStatus(id: string, status: ChangeEvent["status"]) {
  const now = new Date().toISOString();
  memoryChangeEvents = memoryChangeEvents.map((event) =>
    event.id === id
      ? { ...event, status, updatedAt: now, resolvedAt: status === "resolved" ? now : event.resolvedAt }
      : event,
  );
  const event = memoryChangeEvents.find((item) => item.id === id);
  if (!event) {
    return Promise.reject(new Error("change event not found"));
  }
  return Promise.resolve(event);
}
```

- [ ] **Step 3: Add API wrapper**

Create `src/lib/api/changeEvents.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import {
  listMockChangeEvents,
  updateMockChangeEventStatus,
  upsertMockChangeEvent,
} from "@/lib/mock/changeEvents";
import type { ChangeEvent, UpsertChangeEventInput } from "@/lib/types/changeEvents";
import { isInvokeUnavailable } from "./invoke";

export function listChangeEvents() {
  return invoke<ChangeEvent[]>("list_change_events").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return listMockChangeEvents();
    }
    throw error;
  });
}

export function upsertChangeEvent(input: UpsertChangeEventInput) {
  return invoke<ChangeEvent>("upsert_change_event", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return upsertMockChangeEvent(input);
    }
    throw error;
  });
}

export function markChangeEventRead(id: string) {
  return invoke<ChangeEvent>("mark_change_event_read", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "read");
    }
    throw error;
  });
}

export function dismissChangeEvent(id: string) {
  return invoke<ChangeEvent>("dismiss_change_event", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "dismissed");
    }
    throw error;
  });
}

export function resolveChangeEvent(id: string) {
  return invoke<ChangeEvent>("resolve_change_event", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "resolved");
    }
    throw error;
  });
}
```

If this repo does not have `src/lib/api/invoke.ts`, use the existing `isInvokeUnavailable` import pattern from nearby API files.

- [ ] **Step 4: Run TypeScript check**

Run:

```powershell
pnpm.cmd tsc --noEmit
```

Expected:

- PASS.

- [ ] **Step 5: Commit frontend API**

Run:

```powershell
git add -- src/lib/types/changeEvents.ts src/lib/mock/changeEvents.ts src/lib/api/changeEvents.ts src/lib/mock/index.ts
git commit -m "feat: add change event frontend API"
```

Expected:

- Commit succeeds.
- If `src/lib/mock/index.ts` was not modified, omit it from `git add`.

---

## Task 6: Build Change Center Page

**Files:**
- Create: `src/features/changes/changeEventViewModels.ts`
- Create: `src/features/changes/ChangeCenterPage.tsx`

- [ ] **Step 1: Add view model helpers**

Create `src/features/changes/changeEventViewModels.ts`:

```ts
import type { ChangeEvent, ChangeEventStatus, ChangeSeverity } from "@/lib/types/changeEvents";

export type ChangeFilter = {
  severity: "all" | ChangeSeverity;
  status: "active" | "all" | ChangeEventStatus;
  query: string;
};

export const severityLabels: Record<ChangeSeverity, string> = {
  critical: "严重",
  warning: "警告",
  info: "信息",
};

export const severityTone: Record<ChangeSeverity, "error" | "warning" | "info"> = {
  critical: "error",
  warning: "warning",
  info: "info",
};

export const statusLabels: Record<ChangeEventStatus, string> = {
  unread: "未读",
  read: "已读",
  dismissed: "已忽略",
  resolved: "已解决",
};

export const eventTypeLabels: Record<string, string> = {
  balance_low: "余额偏低",
  balance_depleted: "余额耗尽",
  rate_changed: "倍率变化",
  price_changed: "价格变化",
  model_added: "模型新增",
  model_removed: "模型下架",
  key_invalid: "Key 异常",
  collector_failed: "采集失败",
  collector_recovered: "采集恢复",
  route_impacted: "路由受影响",
  station_down: "站点异常",
  station_recovered: "站点恢复",
};

export function filterChangeEvents(events: ChangeEvent[], filter: ChangeFilter) {
  const query = filter.query.trim().toLowerCase();
  return events.filter((event) => {
    if (filter.severity !== "all" && event.severity !== filter.severity) {
      return false;
    }
    if (filter.status === "active") {
      if (event.status === "dismissed" || event.status === "resolved") {
        return false;
      }
    } else if (filter.status !== "all" && event.status !== filter.status) {
      return false;
    }
    if (!query) {
      return true;
    }
    return `${event.title} ${event.message} ${event.eventType} ${event.source} ${event.objectType}`
      .toLowerCase()
      .includes(query);
  });
}

export function unreadRiskCount(events: ChangeEvent[]) {
  return events.filter(
    (event) => event.status === "unread" && (event.severity === "critical" || event.severity === "warning"),
  ).length;
}

export function formatChangeTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function parseJsonObject(value: string | null) {
  if (!value) {
    return null;
  }
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return value;
  }
}
```

- [ ] **Step 2: Add page component**

Create `src/features/changes/ChangeCenterPage.tsx`:

```tsx
import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, CheckCircle2, Eye, RefreshCw, Search, XCircle } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  DataTableLite,
  EmptyState,
  InspectorPanel,
  SegmentedControl,
  SelectControl,
  StatusBadge,
  Toolbar,
  useToast,
  type DataTableColumn,
} from "@/components/ui";
import {
  dismissChangeEvent,
  listChangeEvents,
  markChangeEventRead,
  resolveChangeEvent,
} from "@/lib/api/changeEvents";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import {
  eventTypeLabels,
  filterChangeEvents,
  formatChangeTime,
  parseJsonObject,
  severityLabels,
  severityTone,
  statusLabels,
  unreadRiskCount,
  type ChangeFilter,
} from "./changeEventViewModels";

export function ChangeCenterPage() {
  const toast = useToast();
  const [events, setEvents] = useState<ChangeEvent[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [filter, setFilter] = useState<ChangeFilter>({ severity: "all", status: "active", query: "" });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
  }, []);

  async function refresh(showSuccess = false) {
    setLoading(true);
    setError(null);
    try {
      const nextEvents = await listChangeEvents();
      setEvents(nextEvents);
      setSelectedId((current) => current ?? nextEvents[0]?.id ?? null);
      if (showSuccess) {
        toast.success("变更中心已刷新");
      }
    } catch (requestError) {
      const message = readError(requestError);
      setError(message);
      toast.error("刷新变更中心失败", message);
    } finally {
      setLoading(false);
    }
  }

  async function runAction(action: () => Promise<ChangeEvent>, successMessage: string) {
    setSaving(true);
    try {
      const updated = await action();
      setEvents((current) => current.map((event) => (event.id === updated.id ? updated : event)));
      toast.success(successMessage);
    } catch (requestError) {
      toast.error("更新变更状态失败", readError(requestError));
    } finally {
      setSaving(false);
    }
  }

  const filteredEvents = useMemo(() => filterChangeEvents(events, filter), [events, filter]);
  const selected = filteredEvents.find((event) => event.id === selectedId) ?? filteredEvents[0] ?? null;
  const riskCount = unreadRiskCount(events);

  const columns: DataTableColumn<ChangeEvent>[] = [
    {
      key: "severity",
      header: "级别",
      className: "w-20",
      render: (event) => <StatusBadge tone={severityTone[event.severity]}>{severityLabels[event.severity]}</StatusBadge>,
    },
    {
      key: "event",
      header: "变更",
      render: (event) => (
        <div className="min-w-0">
          <div className="truncate font-semibold text-slate-800">{event.title}</div>
          <div className="truncate text-xs text-muted-foreground">{event.message}</div>
        </div>
      ),
    },
    {
      key: "type",
      header: "类型",
      className: "w-28",
      render: (event) => eventTypeLabels[event.eventType] ?? event.eventType,
    },
    {
      key: "status",
      header: "状态",
      className: "w-24",
      render: (event) => statusLabels[event.status] ?? event.status,
    },
    {
      key: "time",
      header: "时间",
      className: "w-32",
      render: (event) => formatChangeTime(event.detectedAt),
    },
  ];

  return (
    <PageScaffold
      title="变更中心"
      description="记录余额、倍率、价格、模型、Key、采集和路由状态的变化。"
      actions={
        <Button variant="secondary" onClick={() => void refresh(true)} disabled={loading || saving}>
          <RefreshCw className="h-4 w-4" />
          刷新
        </Button>
      }
    >
      <div className="grid gap-[var(--shell-page-gap)]">
        <div className="grid gap-3 md:grid-cols-4">
          <SummaryTile label="未读风险" value={riskCount} tone={riskCount > 0 ? "text-rose-700" : "text-emerald-700"} />
          <SummaryTile label="严重" value={events.filter((event) => event.severity === "critical" && event.status !== "resolved").length} />
          <SummaryTile label="警告" value={events.filter((event) => event.severity === "warning" && event.status !== "resolved").length} />
          <SummaryTile label="信息" value={events.filter((event) => event.severity === "info").length} />
        </div>

        <div className="grid gap-[var(--shell-page-gap)] xl:grid-cols-[minmax(0,1fr)_420px]">
          <div className="min-w-0 overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
            <Toolbar>
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <SegmentedControl
                  value={filter.status}
                  options={[
                    { value: "active", label: "活跃" },
                    { value: "unread", label: "未读" },
                    { value: "resolved", label: "已解决" },
                    { value: "all", label: "全部" },
                  ]}
                  onChange={(status) => setFilter((current) => ({ ...current, status: status as ChangeFilter["status"] }))}
                />
                <SelectControl
                  ariaLabel="变更级别"
                  className={inputClassName}
                  value={filter.severity}
                  options={[
                    { value: "all", label: "全部级别" },
                    { value: "critical", label: "严重" },
                    { value: "warning", label: "警告" },
                    { value: "info", label: "信息" },
                  ]}
                  onChange={(severity) => setFilter((current) => ({ ...current, severity: severity as ChangeFilter["severity"] }))}
                />
                <div className="relative">
                  <Search className="pointer-events-none absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
                  <input
                    className={`${inputClassName} pl-8`}
                    value={filter.query}
                    placeholder="搜索变更 / 对象 / 来源"
                    onChange={(event) => setFilter((current) => ({ ...current, query: event.target.value }))}
                  />
                </div>
              </div>
            </Toolbar>
            {error && <div className="border-b border-rose-100 bg-rose-50 px-3 py-2 text-sm text-rose-700">{error}</div>}
            {filteredEvents.length === 0 ? (
              <EmptyState
                title={loading ? "正在读取变更" : "暂无变更"}
                description="余额、Key、采集、价格、倍率、模型和路由状态变化会在这里形成记录。"
              />
            ) : (
              <DataTableLite
                columns={columns}
                rows={filteredEvents}
                getRowKey={(event) => event.id}
                selectedKey={selected?.id}
                onRowClick={(event) => setSelectedId(event.id)}
                className="rounded-none border-0 shadow-none"
              />
            )}
          </div>

          <InspectorPanel title={selected ? selected.title : "变更详情"} description={selected ? eventTypeLabels[selected.eventType] ?? selected.eventType : "选择一条变更"}>
            {selected ? (
              <div className="space-y-4 p-4">
                <div className="flex flex-wrap items-center gap-2">
                  <StatusBadge tone={severityTone[selected.severity]}>{severityLabels[selected.severity]}</StatusBadge>
                  <StatusBadge tone={selected.status === "unread" ? "warning" : "info"}>{statusLabels[selected.status]}</StatusBadge>
                  <span className="text-xs text-muted-foreground">{formatChangeTime(selected.detectedAt)}</span>
                </div>
                <div className="rounded-[var(--surface-radius)] border border-border bg-slate-50 p-3 text-sm leading-6 text-slate-700">
                  {selected.message}
                </div>
                <JsonBlock title="变化前" value={parseJsonObject(selected.oldValueJson)} />
                <JsonBlock title="变化后" value={parseJsonObject(selected.newValueJson)} />
                <JsonBlock title="影响" value={parseJsonObject(selected.impactJson)} />
                <div className="grid gap-2 text-xs text-muted-foreground">
                  <div>对象：{selected.objectType} / {selected.objectId ?? "-"}</div>
                  <div>来源：{selected.source}</div>
                  <div>Dedupe：{selected.dedupeKey}</div>
                </div>
                <div className="flex flex-wrap justify-end gap-2">
                  <Button variant="outline" disabled={saving || selected.status === "read"} onClick={() => void runAction(() => markChangeEventRead(selected.id), "已标记为已读")}>
                    <Eye className="h-4 w-4" />
                    标记已读
                  </Button>
                  <Button variant="outline" disabled={saving || selected.status === "resolved"} onClick={() => void runAction(() => resolveChangeEvent(selected.id), "已标记为已解决")}>
                    <CheckCircle2 className="h-4 w-4" />
                    解决
                  </Button>
                  <Button variant="danger" disabled={saving || selected.status === "dismissed"} onClick={() => void runAction(() => dismissChangeEvent(selected.id), "已忽略")}>
                    <XCircle className="h-4 w-4" />
                    忽略
                  </Button>
                </div>
              </div>
            ) : (
              <EmptyState title="暂无详情" description="选择一条变更查看变化值和影响范围。" />
            )}
          </InspectorPanel>
        </div>
      </div>
    </PageScaffold>
  );
}

function SummaryTile({ label, value, tone = "text-slate-800" }: { label: string; value: number; tone?: string }) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 shadow-[var(--surface-shadow)]">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={`mt-1 text-2xl font-semibold ${tone}`}>{value}</div>
    </div>
  );
}

function JsonBlock({ title, value }: { title: string; value: unknown }) {
  if (value == null) {
    return null;
  }
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3">
      <div className="text-xs font-semibold text-slate-700">{title}</div>
      <pre className="mt-2 max-h-40 overflow-auto rounded-[var(--surface-radius)] bg-slate-50 p-2 text-[11px] leading-5 text-slate-600">
        {typeof value === "string" ? value : JSON.stringify(value, null, 2)}
      </pre>
    </div>
  );
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const inputClassName =
  "h-8 rounded-[12px] border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
```

- [ ] **Step 3: Run TypeScript check**

Run:

```powershell
pnpm.cmd tsc --noEmit
```

Expected:

- PASS.

- [ ] **Step 4: Commit change center page**

Run:

```powershell
git add -- src/features/changes/changeEventViewModels.ts src/features/changes/ChangeCenterPage.tsx
git commit -m "feat: add change center page"
```

Expected:

- Commit succeeds.

---

## Task 7: Update Navigation, Shell Badge, and Page Names

**Files:**
- Modify: `src/lib/types/navigation.ts`
- Modify: `src/app/routes.tsx`
- Modify: `src/app/App.tsx`
- Modify: `src/components/shell/AppShell.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/pricing/PricingPage.tsx`

- [ ] **Step 1: Update navigation types**

In `src/lib/types/navigation.ts`, ensure route ids include:

```ts
export type AppRouteId =
  | "dashboard"
  | "stations"
  | "keyPool"
  | "routing"
  | "pricing"
  | "channels"
  | "changes"
  | "logs"
  | "settings";

export type AppPageId = AppRouteId | "addProvider" | "collectors";
```

Keep `"collectors"` only as internal page id if `CollectorsPage` still needs to be opened from station details later.

- [ ] **Step 2: Update routes**

Modify `src/app/routes.tsx` route array to this order and labels:

```ts
export const appRoutes: AppRoute[] = [
  {
    id: "dashboard",
    label: "总览",
    description: "当前风险、本地代理和关键运行摘要",
    icon: LayoutDashboard,
  },
  {
    id: "stations",
    label: "中转站资产",
    description: "站点资产、余额、倍率、采集和路由参与状态",
    icon: DatabaseZap,
  },
  {
    id: "keyPool",
    label: "Key 池",
    description: "所有 Station Key 的路由可用性和优先级",
    icon: KeyRound,
  },
  {
    id: "routing",
    label: "路由规则",
    description: "默认策略、模型映射和候选解释",
    icon: GitBranch,
  },
  {
    id: "pricing",
    label: "价格 / 倍率",
    description: "跨站点模型价格、分组倍率和可用性对比",
    icon: BarChart3,
  },
  {
    id: "channels",
    label: "渠道状态",
    description: "Key / Channel 的延迟、成功率和最近状态",
    icon: Radar,
  },
  {
    id: "changes",
    label: "变更中心",
    description: "余额、Key、采集、价格、倍率、模型和路由变化",
    icon: Activity,
  },
  {
    id: "logs",
    label: "请求日志",
    description: "请求、耗时、成本和 fallback 轨迹",
    icon: ClipboardList,
  },
  {
    id: "settings",
    label: "设置",
    description: "本地代理、安全和数据目录",
    icon: Settings,
  },
];
```

Remove the old first-level `collectors` route from this array.

- [ ] **Step 3: Wire route to page**

Modify `src/app/App.tsx`:

```tsx
import { ChangeCenterPage } from "@/features/changes/ChangeCenterPage";
```

Add switch case:

```tsx
case "changes":
  return <ChangeCenterPage />;
```

Keep:

```tsx
case "collectors":
  return <CollectorsPage />;
```

only if TypeScript needs the internal page id. Do not expose it in `appRoutes`.

- [ ] **Step 4: Add shell badge**

Modify `src/components/shell/AppShell.tsx` imports:

```tsx
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { listChangeEvents } from "@/lib/api/changeEvents";
import { unreadRiskCount } from "@/features/changes/changeEventViewModels";
import type { ChangeEvent } from "@/lib/types/changeEvents";
```

Inside `AppShell`, add state:

```tsx
const [changeEvents, setChangeEvents] = useState<ChangeEvent[]>([]);

useEffect(() => {
  void listChangeEvents()
    .then(setChangeEvents)
    .catch(() => setChangeEvents([]));
}, [activeRouteId]);

const changeRiskCount = useMemo(() => unreadRiskCount(changeEvents), [changeEvents]);
```

Inside route button rendering, after `<Icon ... />`, add:

```tsx
{route.id === "changes" && changeRiskCount > 0 && (
  <span className="absolute right-1 top-1 min-w-4 rounded-full bg-rose-600 px-1 text-[10px] font-semibold leading-4 text-white">
    {changeRiskCount > 99 ? "99+" : changeRiskCount}
  </span>
)}
```

Update the button class to include `relative`:

```tsx
"relative flex h-10 w-10 cursor-pointer items-center justify-center rounded-[var(--surface-radius)] transition-colors"
```

- [ ] **Step 5: Rename page titles**

In `src/features/dashboard/DashboardPage.tsx`, change:

```tsx
title="代理工作台"
```

to:

```tsx
title="总览"
```

and change description to:

```tsx
description="优先展示当前风险、本地代理状态、今日请求、失败率和成本摘要。"
```

In `src/features/stations/StationsPage.tsx`, change page title to `中转站资产`.

In `src/features/pricing/PricingPage.tsx`, change page title to `价格 / 倍率`.

- [ ] **Step 6: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 7: Commit navigation upgrade**

Run:

```powershell
git add -- src/lib/types/navigation.ts src/app/routes.tsx src/app/App.tsx src/components/shell/AppShell.tsx src/features/dashboard/DashboardPage.tsx src/features/stations/StationsPage.tsx src/features/pricing/PricingPage.tsx
git commit -m "feat: upgrade navigation information architecture"
```

Expected:

- Commit succeeds.

---

## Task 8: Upgrade Dashboard to Risk-First Overview

**Files:**
- Modify: `src/features/dashboard/DashboardPage.tsx`

- [ ] **Step 1: Add change event data loading**

Modify imports:

```tsx
import { listChangeEvents } from "@/lib/api/changeEvents";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import { formatChangeTime, severityLabels, severityTone, unreadRiskCount } from "@/features/changes/changeEventViewModels";
```

Add state:

```tsx
const [changeEvents, setChangeEvents] = useState<ChangeEvent[]>([]);
```

Update the initial `Promise.all` to include `listChangeEvents()`:

```tsx
void Promise.all([
  getProxyStatus(),
  listRequestLogs(),
  listKeyPoolItems(),
  listBalanceSnapshots(),
  getSettings(),
  listChangeEvents(),
])
  .then(([status, logs, keys, balances, nextSettings, changes]) => {
    setProxyStatus(status);
    setRequestLogs(logs);
    setKeyPoolItems(keys);
    setBalanceSnapshots(balances);
    setSettings(nextSettings);
    setChangeEvents(changes);
  })
```

- [ ] **Step 2: Add risk-first derived values**

Add:

```tsx
const activeRiskEvents = useMemo(
  () =>
    changeEvents.filter(
      (event) =>
        (event.severity === "critical" || event.severity === "warning") &&
        event.status !== "dismissed" &&
        event.status !== "resolved",
    ),
  [changeEvents],
);
const unreadRisks = unreadRiskCount(changeEvents);
const criticalRisks = activeRiskEvents.filter((event) => event.severity === "critical").length;
```

- [ ] **Step 3: Replace generic recent activity priority**

Add a `SectionCard` near the top after the proxy status block:

```tsx
<SectionCard
  title="当前风险"
  description="来自变更中心的未解决严重 / 警告事件。"
  action={<StatusBadge tone={unreadRisks > 0 ? "warning" : "healthy"}>{unreadRisks > 0 ? `${unreadRisks} 未读` : "无未读风险"}</StatusBadge>}
>
  {activeRiskEvents.length === 0 ? (
    <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
      当前没有未解决的严重或警告变更。
    </div>
  ) : (
    <div className="grid gap-2">
      {activeRiskEvents.slice(0, 5).map((event) => (
        <ObjectRow
          key={event.id}
          icon={<AlertTriangle className="h-4 w-4" />}
          title={event.title}
          subtitle={`${event.message} · ${formatChangeTime(event.detectedAt)}`}
          badges={<StatusBadge tone={severityTone[event.severity]}>{severityLabels[event.severity]}</StatusBadge>}
          metrics={[{ label: "来源", value: event.source }]}
        />
      ))}
    </div>
  )}
</SectionCard>
```

Keep existing proxy, today metrics, key route queue, and station health blocks. Remove duplicate balance-change activity if it becomes redundant with `当前风险`.

- [ ] **Step 4: Ensure summary metrics reflect risk**

In the existing `MetricPanel`, add or adjust metrics so one metric is:

```tsx
{
  label: "当前风险",
  value: `${activeRiskEvents.length}`,
  detail: `${criticalRisks} 严重`,
  icon: AlertTriangle,
  tone: activeRiskEvents.length > 0 ? "warning" : "good",
}
```

- [ ] **Step 5: Run frontend checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 6: Commit dashboard overview**

Run:

```powershell
git add -- src/features/dashboard/DashboardPage.tsx
git commit -m "feat: make overview risk first"
```

Expected:

- Commit succeeds.

---

## Task 9: Build Station Asset View Models

**Files:**
- Create: `src/features/stations/stationAssetViewModels.ts`
- Test by TypeScript compile

- [ ] **Step 1: Create station asset view model**

Create `src/features/stations/stationAssetViewModels.ts`:

```ts
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { StationKey } from "@/lib/types/stationKeys";
import type { CollectorSnapshot } from "@/lib/types/collector";
import type { Station } from "@/lib/types/stations";

export type RateChip = {
  label: string;
  value: string;
  tone: "neutral" | "good" | "warning";
};

export type StationAssetRow = {
  station: Station;
  enabledKeyCount: number;
  warningKeyCount: number;
  latestBalance: BalanceSnapshot | null;
  latestSnapshot: CollectorSnapshot | null;
  riskEvents: ChangeEvent[];
  rateChips: RateChip[];
  participatesInRouting: boolean;
};

export function buildStationAssetRows({
  stations,
  keysByStation,
  balances,
  snapshotsByStation,
  changes,
}: {
  stations: Station[];
  keysByStation: Map<string, StationKey[]>;
  balances: BalanceSnapshot[];
  snapshotsByStation: Map<string, CollectorSnapshot | null>;
  changes: ChangeEvent[];
}): StationAssetRow[] {
  const latestBalanceByStation = latestBalanceMap(balances);
  return stations.map((station) => {
    const keys = keysByStation.get(station.id) ?? [];
    const riskEvents = changes.filter(
      (event) =>
        event.stationId === station.id &&
        event.status !== "dismissed" &&
        event.status !== "resolved" &&
        (event.severity === "critical" || event.severity === "warning"),
    );
    return {
      station,
      enabledKeyCount: keys.filter((key) => key.enabled).length,
      warningKeyCount: keys.filter((key) => key.status === "warning" || key.status === "error").length,
      latestBalance: latestBalanceByStation.get(station.id) ?? null,
      latestSnapshot: snapshotsByStation.get(station.id) ?? null,
      riskEvents,
      rateChips: extractRateChips(snapshotsByStation.get(station.id) ?? null),
      participatesInRouting: station.enabled && keys.some((key) => key.enabled),
    };
  });
}

function latestBalanceMap(balances: BalanceSnapshot[]) {
  const map = new Map<string, BalanceSnapshot>();
  for (const balance of balances) {
    if (balance.scope !== "station") {
      continue;
    }
    const current = map.get(balance.stationId);
    if (!current || toTime(balance.updatedAt) > toTime(current.updatedAt)) {
      map.set(balance.stationId, balance);
    }
  }
  return map;
}

export function extractRateChips(snapshot: CollectorSnapshot | null): RateChip[] {
  const rates = Array.isArray(snapshot?.normalizedJson.rateMultipliers)
    ? (snapshot?.normalizedJson.rateMultipliers as Array<Record<string, unknown>>)
    : [];
  return rates.slice(0, 3).map((rate) => {
    const group = String(rate.groupName ?? rate.group ?? rate.name ?? "default");
    const multiplier = Number(rate.multiplier ?? rate.rate ?? rate.value ?? 1);
    return {
      label: group,
      value: Number.isFinite(multiplier) ? `${multiplier.toFixed(2)}x` : "-",
      tone: !Number.isFinite(multiplier) ? "neutral" : multiplier > 1 ? "warning" : multiplier < 1 ? "good" : "neutral",
    };
  });
}

export function formatStationBalance(row: StationAssetRow) {
  const value = row.latestBalance?.value ?? row.station.balanceCny;
  if (value == null) {
    return "未采集";
  }
  const currency = row.latestBalance?.currency ?? "CNY";
  return `${currency} ${value.toFixed(2)}`;
}

export function stationRiskTone(row: StationAssetRow): "healthy" | "warning" | "error" | "disabled" | "info" {
  if (!row.station.enabled) {
    return "disabled";
  }
  if (row.riskEvents.some((event) => event.severity === "critical")) {
    return "error";
  }
  if (row.riskEvents.some((event) => event.severity === "warning") || row.warningKeyCount > 0) {
    return "warning";
  }
  if (row.station.status === "healthy") {
    return "healthy";
  }
  if (row.station.status === "error") {
    return "error";
  }
  if (row.station.status === "warning") {
    return "warning";
  }
  return "info";
}

function toTime(value: string | null) {
  if (!value) {
    return 0;
  }
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 0 : date.getTime();
}
```

- [ ] **Step 2: Run TypeScript check**

Run:

```powershell
pnpm.cmd tsc --noEmit
```

Expected:

- PASS.

- [ ] **Step 3: Commit station view model**

Run:

```powershell
git add -- src/features/stations/stationAssetViewModels.ts
git commit -m "feat: add station asset view models"
```

Expected:

- Commit succeeds.

---

## Task 10: Upgrade Stations Page to High-Density Asset Table and Drawer

**Files:**
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/stations/components/StationDetailPanel.tsx` if reused

- [ ] **Step 1: Add required data loading**

In `StationsPage.tsx`, add imports:

```tsx
import { listBalanceSnapshots } from "@/lib/api/economics";
import { listChangeEvents } from "@/lib/api/changeEvents";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import {
  buildStationAssetRows,
  formatStationBalance,
  stationRiskTone,
  type StationAssetRow,
} from "./stationAssetViewModels";
```

Add state:

```tsx
const [balanceSnapshots, setBalanceSnapshots] = useState<BalanceSnapshot[]>([]);
const [changeEvents, setChangeEvents] = useState<ChangeEvent[]>([]);
const [drawerStationId, setDrawerStationId] = useState<string | null>(null);
```

- [ ] **Step 2: Load cross-station support data**

Update `refreshStations` to also load balances and changes:

```tsx
const [nextStations, nextBalances, nextChanges] = await Promise.all([
  listStations(),
  listBalanceSnapshots(),
  listChangeEvents(),
]);
setStations(nextStations);
setBalanceSnapshots(nextBalances);
setChangeEvents(nextChanges);
```

Keep existing selected station logic.

- [ ] **Step 3: Build asset rows**

Add memo:

```tsx
const keysByStation = useMemo(() => {
  const map = new Map<string, StationKey[]>();
  if (activeDialogStation && stationKeys.length > 0) {
    map.set(activeDialogStation.id, stationKeys);
  }
  return map;
}, [activeDialogStation, stationKeys]);

const snapshotsByStation = useMemo(() => {
  const map = new Map<string, CollectorSnapshot | null>();
  if (detailStation && snapshot) {
    map.set(detailStation.id, snapshot);
  }
  return map;
}, [detailStation, snapshot]);

const stationAssetRows = useMemo(
  () =>
    buildStationAssetRows({
      stations,
      keysByStation,
      balances: balanceSnapshots,
      snapshotsByStation,
      changes: changeEvents,
    }),
  [balanceSnapshots, changeEvents, keysByStation, snapshotsByStation, stations],
);
```

This first pass can use station `keyCount` for table counts until all per-station keys are loaded. Do not fetch every station's keys in a loop unless needed; keep performance tame.

- [ ] **Step 4: Replace row stack with table-like asset rows**

Replace the `SortableContext` row rendering block with a non-draggable high-density table for this first IA upgrade:

```tsx
<div className="overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
  <div className="grid grid-cols-[minmax(160px,1.3fr)_120px_minmax(180px,1.2fr)_110px_180px_110px_110px_130px_96px] border-b border-border bg-slate-50 px-3 py-2 text-xs font-semibold text-muted-foreground">
    <div>站点</div>
    <div>类型</div>
    <div>Base URL</div>
    <div>余额</div>
    <div>分组倍率</div>
    <div>Key</div>
    <div>健康</div>
    <div>更新时间</div>
    <div className="text-right">路由</div>
  </div>
  <div className="divide-y divide-border">
    {stationAssetRows.map((row) => (
      <button
        key={row.station.id}
        type="button"
        className="grid w-full grid-cols-[minmax(160px,1.3fr)_120px_minmax(180px,1.2fr)_110px_180px_110px_110px_130px_96px] items-center gap-2 px-3 py-2.5 text-left text-sm hover:bg-slate-50"
        onClick={() => {
          setSelectedStationId(row.station.id);
          setDrawerStationId(row.station.id);
          void refreshExtras(row.station.id);
        }}
      >
        <div className="min-w-0">
          <div className="truncate font-semibold text-slate-800">{row.station.name}</div>
          <div className="truncate text-xs text-muted-foreground">{row.riskEvents[0]?.title ?? row.station.note ?? "暂无风险摘要"}</div>
        </div>
        <div>{stationTypeLabels[row.station.stationType]}</div>
        <code className="truncate text-xs text-slate-600">{row.station.baseUrl}</code>
        <div>{formatStationBalance(row)}</div>
        <div className="flex min-w-0 flex-wrap gap-1">
          {row.rateChips.length === 0 ? (
            <span className="text-xs text-muted-foreground">未采集</span>
          ) : (
            row.rateChips.map((chip) => (
              <span key={`${row.station.id}-${chip.label}`} className="rounded-full border border-border bg-slate-50 px-2 py-0.5 text-[11px] text-slate-700">
                {chip.label} {chip.value}
              </span>
            ))
          )}
        </div>
        <div>{row.enabledKeyCount || row.station.keyCount} / {row.station.keyCount}</div>
        <StatusBadge tone={stationRiskTone(row)}>{stationStatusLabels[row.station.status]}</StatusBadge>
        <div className="text-xs text-muted-foreground">{formatNullableTime(row.station.updatedAt)}</div>
        <div className="text-right">
          <StatusBadge tone={row.participatesInRouting ? "healthy" : "disabled"}>
            {row.participatesInRouting ? "参与" : "暂停"}
          </StatusBadge>
        </div>
      </button>
    ))}
  </div>
</div>
```

Add helper if not already present:

```tsx
function formatNullableTime(value: string | null) {
  if (!value) {
    return "未记录";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
}
```

- [ ] **Step 5: Add right-side drawer**

Add drawer rendering near the bottom of `StationsPage`:

```tsx
{drawerStationId && detailStation && (
  <div className="fixed inset-y-0 right-0 z-40 w-[min(560px,calc(100vw-72px))] border-l border-border bg-white shadow-2xl">
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-slate-900">{detailStation.name}</div>
          <div className="truncate text-xs text-muted-foreground">{detailStation.baseUrl}</div>
        </div>
        <Button variant="outline" onClick={() => setDrawerStationId(null)}>关闭</Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <DetailBody
          activeDialogStation={detailStation}
          credentials={credentials}
          keyCountLabel={`${detailStation.keyCount} keys`}
          snapshot={snapshot}
          snapshots={snapshots}
          stationKeys={stationKeys}
          onDeleteKey={handleDeleteKey}
          onEditKey={(key) => {
            setKeyForm(keyToForm(key));
            setKeyDialogOpen(true);
          }}
        />
      </div>
    </div>
  </div>
)}
```

Keep existing create/edit dialogs. The drawer is for details, not create/edit forms.

- [ ] **Step 6: Remove detail dialog path**

Change `openDetail` to:

```tsx
const openDetail = useCallback((station: Station) => {
  setDrawerStationId(station.id);
  setDetailStationId(station.id);
  setSelectedStationId(station.id);
  setError(null);
  void refreshExtras(station.id);
}, [refreshExtras]);
```

Ensure `dialogMode === "detail"` is no longer required to show details. Keep `DialogMode` values only for create/edit if possible:

```ts
type DialogMode = "create" | "edit" | null;
```

- [ ] **Step 7: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 8: Commit station asset page**

Run:

```powershell
git add -- src/features/stations/StationsPage.tsx src/features/stations/components/StationDetailPanel.tsx
git commit -m "feat: upgrade stations to asset view"
```

Expected:

- Commit succeeds.
- If `StationDetailPanel.tsx` was not modified, omit it.

---

## Task 11: Add Pricing and Rate Matrix Helpers

**Files:**
- Create: `src/features/pricing/rateSnapshotParser.ts`
- Create: `src/features/pricing/pricingMatrix.ts`

- [ ] **Step 1: Add rate snapshot parser**

Create `src/features/pricing/rateSnapshotParser.ts`:

```ts
import type { CollectorSnapshot } from "@/lib/types/collector";

export type RateMultiplierRow = {
  stationId: string;
  groupName: string;
  multiplier: number | null;
  source: string;
  updatedAt: string;
};

export function parseRateMultipliers(snapshot: CollectorSnapshot | null): RateMultiplierRow[] {
  if (!snapshot) {
    return [];
  }
  const rawRates = Array.isArray(snapshot.normalizedJson.rateMultipliers)
    ? (snapshot.normalizedJson.rateMultipliers as Array<Record<string, unknown>>)
    : [];
  return rawRates.map((rate) => {
    const multiplier = Number(rate.multiplier ?? rate.rate ?? rate.value);
    return {
      stationId: snapshot.stationId,
      groupName: String(rate.groupName ?? rate.group ?? rate.name ?? "default"),
      multiplier: Number.isFinite(multiplier) ? multiplier : null,
      source: snapshot.source,
      updatedAt: snapshot.fetchedAt,
    };
  });
}
```

- [ ] **Step 2: Add matrix helpers**

Create `src/features/pricing/pricingMatrix.ts`:

```ts
import type { PricingRule } from "@/lib/types/economics";
import type { Station } from "@/lib/types/stations";
import type { RateMultiplierRow } from "./rateSnapshotParser";

export type PriceMatrixCell = {
  stationId: string;
  model: string;
  groupName: string | null;
  inputPrice: number | null;
  outputPrice: number | null;
  fixedPrice: number | null;
  currency: string;
  updatedAt: string;
  isCheapestOutput: boolean;
  available: boolean;
};

export type PriceMatrixRow = {
  model: string;
  cells: PriceMatrixCell[];
};

export type RateMatrixRow = {
  groupName: string;
  cells: Array<{
    stationId: string;
    multiplier: number | null;
    updatedAt: string;
  }>;
};

export function buildPriceMatrix(rules: PricingRule[], stations: Station[]): PriceMatrixRow[] {
  const enabledRules = rules.filter((rule) => rule.enabled);
  const models = Array.from(new Set(enabledRules.map((rule) => rule.model))).sort((a, b) => a.localeCompare(b));
  return models.map((model) => {
    const modelRules = enabledRules.filter((rule) => rule.model === model);
    const cheapest = cheapestOutput(modelRules);
    return {
      model,
      cells: stations.map((station) => {
        const rule = newestRule(modelRules.filter((item) => item.stationId === station.id));
        return {
          stationId: station.id,
          model,
          groupName: rule?.groupName ?? null,
          inputPrice: rule?.inputPrice ?? null,
          outputPrice: rule?.outputPrice ?? null,
          fixedPrice: rule?.fixedPrice ?? null,
          currency: rule?.currency ?? "-",
          updatedAt: rule?.updatedAt ?? "",
          isCheapestOutput: Boolean(rule && cheapest && rule.id === cheapest.id),
          available: Boolean(rule),
        };
      }),
    };
  });
}

export function buildRateMatrix(rates: RateMultiplierRow[], stations: Station[]): RateMatrixRow[] {
  const groupNames = Array.from(new Set(rates.map((rate) => rate.groupName))).sort((a, b) => a.localeCompare(b));
  return groupNames.map((groupName) => ({
    groupName,
    cells: stations.map((station) => {
      const newest = newestRate(rates.filter((rate) => rate.stationId === station.id && rate.groupName === groupName));
      return {
        stationId: station.id,
        multiplier: newest?.multiplier ?? null,
        updatedAt: newest?.updatedAt ?? "",
      };
    }),
  }));
}

function cheapestOutput(rules: PricingRule[]) {
  return rules.reduce<PricingRule | null>((best, rule) => {
    const value = comparablePrice(rule);
    if (!Number.isFinite(value)) {
      return best;
    }
    if (!best || value < comparablePrice(best)) {
      return rule;
    }
    return best;
  }, null);
}

function comparablePrice(rule: PricingRule) {
  return rule.outputPrice ?? rule.inputPrice ?? rule.fixedPrice ?? Number.POSITIVE_INFINITY;
}

function newestRule(rules: PricingRule[]) {
  return [...rules].sort((a, b) => toTime(b.updatedAt) - toTime(a.updatedAt))[0] ?? null;
}

function newestRate(rates: RateMultiplierRow[]) {
  return [...rates].sort((a, b) => toTime(b.updatedAt) - toTime(a.updatedAt))[0] ?? null;
}

function toTime(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? 0 : date.getTime();
}
```

- [ ] **Step 3: Run TypeScript check**

Run:

```powershell
pnpm.cmd tsc --noEmit
```

Expected:

- PASS.

- [ ] **Step 4: Commit matrix helpers**

Run:

```powershell
git add -- src/features/pricing/rateSnapshotParser.ts src/features/pricing/pricingMatrix.ts
git commit -m "feat: add pricing comparison matrix helpers"
```

Expected:

- Commit succeeds.

---

## Task 12: Upgrade Pricing Page to Price / Rate Comparison

**Files:**
- Modify: `src/features/pricing/PricingPage.tsx`

- [ ] **Step 1: Load collector snapshots for rate matrix**

Add imports:

```tsx
import { getLatestCollectorSnapshot } from "@/lib/api/collector";
import { buildPriceMatrix, buildRateMatrix } from "./pricingMatrix";
import { parseRateMultipliers } from "./rateSnapshotParser";
```

Add view mode state:

```tsx
const [viewMode, setViewMode] = useState<"prices" | "rates" | "availability">("prices");
const [rateRows, setRateRows] = useState<ReturnType<typeof parseRateMultipliers>>([]);
```

Update `refresh` after stations load:

```tsx
const snapshots = await Promise.all(nextStations.map((station) => getLatestCollectorSnapshot(station.id)));
setRateRows(snapshots.flatMap(parseRateMultipliers));
```

- [ ] **Step 2: Build matrices**

Add memos:

```tsx
const priceMatrix = useMemo(() => buildPriceMatrix(filteredRows, stations), [filteredRows, stations]);
const rateMatrix = useMemo(() => buildRateMatrix(rateRows, stations), [rateRows, stations]);
```

- [ ] **Step 3: Replace primary table with segmented matrix views**

In the main `SectionCard`, set title:

```tsx
<SectionCard title="跨站点对比" description="按模型、分组倍率和可用性比较中转站。">
```

Add segmented control in `Toolbar`:

```tsx
<SegmentedControl
  value={viewMode}
  options={[
    { value: "prices", label: "模型价格" },
    { value: "rates", label: "分组倍率" },
    { value: "availability", label: "模型可用性" },
  ]}
  onChange={(value) => setViewMode(value as typeof viewMode)}
/>
```

Render views:

```tsx
{viewMode === "prices" && (
  <MatrixTable
    rowHeader="模型"
    stations={stations}
    rows={priceMatrix.map((row) => ({
      key: row.model,
      label: row.model,
      cells: row.cells.map((cell) => ({
        stationId: cell.stationId,
        content: cell.available ? formatPriceCell(cell) : "不可用",
        tone: cell.isCheapestOutput ? "good" : cell.available ? "neutral" : "muted",
      })),
    }))}
  />
)}
{viewMode === "rates" && (
  <MatrixTable
    rowHeader="分组"
    stations={stations}
    rows={rateMatrix.map((row) => ({
      key: row.groupName,
      label: row.groupName,
      cells: row.cells.map((cell) => ({
        stationId: cell.stationId,
        content: cell.multiplier == null ? "未采集" : `${cell.multiplier.toFixed(2)}x`,
        tone: cell.multiplier == null ? "muted" : cell.multiplier > 1 ? "warning" : cell.multiplier < 1 ? "good" : "neutral",
      })),
    }))}
  />
)}
{viewMode === "availability" && (
  <MatrixTable
    rowHeader="模型"
    stations={stations}
    rows={priceMatrix.map((row) => ({
      key: row.model,
      label: row.model,
      cells: row.cells.map((cell) => ({
        stationId: cell.stationId,
        content: cell.available ? "可用" : "不可用",
        tone: cell.available ? "good" : "muted",
      })),
    }))}
  />
)}
```

- [ ] **Step 4: Add matrix components**

Add below page component:

```tsx
type MatrixTone = "good" | "warning" | "neutral" | "muted";

function MatrixTable({
  rowHeader,
  stations,
  rows,
}: {
  rowHeader: string;
  stations: Station[];
  rows: Array<{
    key: string;
    label: string;
    cells: Array<{ stationId: string; content: string; tone: MatrixTone }>;
  }>;
}) {
  if (rows.length === 0) {
    return <EmptyState title="暂无对比数据" description="采集价格、倍率或模型后，这里会显示跨站点矩阵。" />;
  }
  return (
    <div className="overflow-auto">
      <div
        className="grid min-w-[760px] border-b border-border bg-slate-50 text-xs font-semibold text-muted-foreground"
        style={{ gridTemplateColumns: `180px repeat(${stations.length}, minmax(132px, 1fr))` }}
      >
        <div className="px-3 py-2">{rowHeader}</div>
        {stations.map((station) => (
          <div key={station.id} className="truncate px-3 py-2">{station.name}</div>
        ))}
      </div>
      <div className="min-w-[760px] divide-y divide-border">
        {rows.map((row) => (
          <div
            key={row.key}
            className="grid text-sm"
            style={{ gridTemplateColumns: `180px repeat(${stations.length}, minmax(132px, 1fr))` }}
          >
            <div className="truncate px-3 py-2.5 font-semibold text-slate-800">{row.label}</div>
            {stations.map((station) => {
              const cell = row.cells.find((item) => item.stationId === station.id);
              return (
                <div key={`${row.key}-${station.id}`} className={`px-3 py-2.5 ${matrixToneClassName(cell?.tone ?? "muted")}`}>
                  {cell?.content ?? "无"}
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

function matrixToneClassName(tone: MatrixTone) {
  if (tone === "good") {
    return "bg-emerald-50 text-emerald-700";
  }
  if (tone === "warning") {
    return "bg-amber-50 text-amber-700";
  }
  if (tone === "muted") {
    return "text-muted-foreground";
  }
  return "text-slate-700";
}

function formatPriceCell(cell: {
  inputPrice: number | null;
  outputPrice: number | null;
  fixedPrice: number | null;
  currency: string;
}) {
  const output = cell.outputPrice ?? cell.inputPrice ?? cell.fixedPrice;
  return output == null ? "暂无价格" : `${cell.currency} ${output.toFixed(4)}`;
}
```

- [ ] **Step 5: Keep inspector useful**

Change inspector title from `${selected.model} inspector` to:

```tsx
title={selected ? `${selected.model} 对比详情` : "对比详情"}
```

Keep the same-model comparison table as secondary detail. It is useful, just no longer the primary page.

- [ ] **Step 6: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 7: Commit pricing matrix page**

Run:

```powershell
git add -- src/features/pricing/PricingPage.tsx
git commit -m "feat: upgrade pricing to comparison matrix"
```

Expected:

- Commit succeeds.

---

## Task 13: Add Price, Rate, and Model Diff Events

**Files:**
- Modify: `src-tauri/src/services/change_events.rs`
- Modify: `src-tauri/src/services/database.rs`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add price change event builder**

In `src-tauri/src/services/change_events.rs`, add:

```rust
pub fn price_changed_event(
    station_id: &str,
    pricing_rule_id: &str,
    model: &str,
    group_name: Option<&str>,
    old_output_price: Option<f64>,
    new_output_price: Option<f64>,
    currency: &str,
) -> Option<UpsertChangeEventInput> {
    if old_output_price == new_output_price {
        return None;
    }
    let increased = match (old_output_price, new_output_price) {
        (Some(old), Some(new)) => new > old,
        _ => false,
    };
    Some(UpsertChangeEventInput {
        severity: if increased { SEVERITY_WARNING } else { SEVERITY_INFO }.to_string(),
        event_type: "price_changed".to_string(),
        title: if increased { "价格变贵" } else { "价格变化" }.to_string(),
        message: format!("模型 {model} 输出价格发生变化"),
        object_type: "pricing_rule".to_string(),
        object_id: Some(pricing_rule_id.to_string()),
        station_id: Some(station_id.to_string()),
        station_key_id: None,
        pricing_rule_id: Some(pricing_rule_id.to_string()),
        request_log_id: None,
        old_value_json: Some(json!({ "outputPrice": old_output_price, "currency": currency }).to_string()),
        new_value_json: Some(json!({ "outputPrice": new_output_price, "currency": currency }).to_string()),
        impact_json: Some(json!({ "cheapFirstMayChange": true }).to_string()),
        dedupe_key: pricing_dedupe_key(station_id, group_name, model),
        source: "pricing".to_string(),
    })
}
```

- [ ] **Step 2: Detect pricing changes in upsert pricing helper**

In the existing pricing upsert helper:

1. Query the newest existing rule for same `station_id`, `group_name`, and `model` before insert/update.
2. After saving the new rule, call `price_changed_event`.

Use this query:

```rust
let previous_rule = connection
    .query_row(
        "SELECT id, station_id, group_name, tier_label, model, input_price, output_price, fixed_price,
                currency, unit, price_type, source, confidence, enabled, note, collected_at, created_at, updated_at
         FROM pricing_rules
         WHERE station_id = ?1 AND COALESCE(group_name, '') = COALESCE(?2, '') AND model = ?3
         ORDER BY updated_at DESC
         LIMIT 1",
        rusqlite::params![input.station_id, input.group_name, input.model],
        row_to_pricing_rule,
    )
    .optional()
    .map_err(|error| format!("读取旧价格失败: {error}"))?;
```

After saved row:

```rust
if let Some(previous) = previous_rule {
    if let Some(event) = crate::services::change_events::price_changed_event(
        &saved.station_id,
        &saved.id,
        &saved.model,
        saved.group_name.as_deref(),
        previous.output_price,
        saved.output_price,
        &saved.currency,
    ) {
        let _ = upsert_change_event_in_connection(connection, event);
    }
}
```

- [ ] **Step 3: Add model added/removed events after collector snapshot diff**

In collector snapshot insertion, compare previous latest snapshot for the same station before insert. Add helpers in `database.rs`:

```rust
fn models_from_snapshot_json(value: &str) -> Vec<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(value).unwrap_or(serde_json::Value::Null);
    parsed
        .get("models")
        .and_then(|models| models.as_array())
        .map(|models| {
            models
                .iter()
                .filter_map(|model| model.as_str().map(ToString::to_string).or_else(|| model.get("id").and_then(|id| id.as_str()).map(ToString::to_string)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}
```

After saving new snapshot:

```rust
let previous_models = previous_snapshot
    .as_ref()
    .map(|snapshot| models_from_snapshot_json(&snapshot.normalized_json))
    .unwrap_or_default();
let next_models = models_from_snapshot_json(&saved.normalized_json);
for model in next_models.iter().filter(|model| !previous_models.contains(model)) {
    let event = UpsertChangeEventInput {
        severity: crate::services::change_events::SEVERITY_INFO.to_string(),
        event_type: "model_added".to_string(),
        title: "模型新增".to_string(),
        message: format!("站点新增模型 {model}"),
        object_type: "station".to_string(),
        object_id: Some(saved.station_id.clone()),
        station_id: Some(saved.station_id.clone()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json: Some(serde_json::json!({ "model": model }).to_string()),
        impact_json: None,
        dedupe_key: crate::services::change_events::model_dedupe_key(&saved.station_id, "model_added", model),
        source: "collector".to_string(),
    };
    let _ = upsert_change_event_in_connection(connection, event);
}
for model in previous_models.iter().filter(|model| !next_models.contains(model)) {
    let event = UpsertChangeEventInput {
        severity: crate::services::change_events::SEVERITY_WARNING.to_string(),
        event_type: "model_removed".to_string(),
        title: "模型下架".to_string(),
        message: format!("站点下架模型 {model}"),
        object_type: "station".to_string(),
        object_id: Some(saved.station_id.clone()),
        station_id: Some(saved.station_id.clone()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: Some(serde_json::json!({ "model": model }).to_string()),
        new_value_json: None,
        impact_json: Some(serde_json::json!({ "routingRisk": "model_candidates_may_change" }).to_string()),
        dedupe_key: crate::services::change_events::model_dedupe_key(&saved.station_id, "model_removed", model),
        source: "collector".to_string(),
    };
    let _ = upsert_change_event_in_connection(connection, event);
}
```

Use actual field names on the Rust `CollectorSnapshot` struct. If JSON is stored as `serde_json::Value` instead of `String`, adapt `models_from_snapshot_json` to accept that type.

- [ ] **Step 4: Add tests for price change**

Add:

```rust
#[test]
fn pricing_change_creates_warning_when_price_increases() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = database
        .create_station(CreateStationInput {
            name: "Price Relay".to_string(),
            station_type: "sub2api".to_string(),
            base_url: "https://price.example/v1".to_string(),
            api_key: "sk-test".to_string(),
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: Some(10.0),
            note: None,
        })
        .expect("station");
    database
        .upsert_pricing_rule(UpsertPricingRuleInput {
            station_id: station.id.clone(),
            group_name: Some("default".to_string()),
            tier_label: None,
            model: "gpt-test".to_string(),
            input_price: Some(1.0),
            output_price: Some(2.0),
            fixed_price: None,
            currency: "USD".to_string(),
            unit: "1M tokens".to_string(),
            price_type: "token".to_string(),
            source: "test".to_string(),
            confidence: 1.0,
            enabled: true,
            note: None,
            collected_at: None,
        })
        .expect("old price");
    database
        .upsert_pricing_rule(UpsertPricingRuleInput {
            station_id: station.id.clone(),
            group_name: Some("default".to_string()),
            tier_label: None,
            model: "gpt-test".to_string(),
            input_price: Some(1.0),
            output_price: Some(3.0),
            fixed_price: None,
            currency: "USD".to_string(),
            unit: "1M tokens".to_string(),
            price_type: "token".to_string(),
            source: "test".to_string(),
            confidence: 1.0,
            enabled: true,
            note: None,
            collected_at: None,
        })
        .expect("new price");
    let events = database.list_change_events().expect("events");
    assert!(events.iter().any(|event| event.event_type == "price_changed" && event.severity == "warning"));
}
```

Adjust input type names to current code if needed.

- [ ] **Step 5: Run tests and checks**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml pricing_change_creates_warning_when_price_increases --lib
cargo test --manifest-path .\src-tauri\Cargo.toml change_event --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 6: Commit diff events**

Run:

```powershell
git add -- src-tauri/src/services/change_events.rs src-tauri/src/services/database.rs
git commit -m "feat: track pricing and model change events"
```

Expected:

- Commit succeeds.

---

## Task 14: Link Change Events into Station and Pricing Details

**Files:**
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/pricing/PricingPage.tsx`

- [ ] **Step 1: Add station drawer change section**

In `StationsPage.tsx`, inside `DetailBody` props add:

```ts
changeEvents: ChangeEvent[];
```

Pass:

```tsx
changeEvents={changeEvents.filter((event) => event.stationId === detailStation.id)}
```

Inside `DetailBody`, add section:

```tsx
<SectionBlock title="关联变更">
  {changeEvents.length === 0 ? (
    <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
      暂无关联变更。
    </div>
  ) : (
    <div className="space-y-2">
      {changeEvents.slice(0, 6).map((event) => (
        <div key={event.id} className="rounded-[var(--surface-radius)] border border-border bg-white p-3 text-sm shadow-[var(--surface-shadow)]">
          <div className="flex items-center justify-between gap-2">
            <span className="font-medium text-slate-800">{event.title}</span>
            <StatusBadge tone={event.severity === "critical" ? "error" : event.severity === "warning" ? "warning" : "info"}>
              {event.severity}
            </StatusBadge>
          </div>
          <div className="mt-1 text-xs text-muted-foreground">{event.message}</div>
        </div>
      ))}
    </div>
  )}
</SectionBlock>
```

- [ ] **Step 2: Add pricing page change data**

In `PricingPage.tsx`, import:

```tsx
import { listChangeEvents } from "@/lib/api/changeEvents";
import type { ChangeEvent } from "@/lib/types/changeEvents";
```

Add state:

```tsx
const [changeEvents, setChangeEvents] = useState<ChangeEvent[]>([]);
```

Update `refresh` to load changes:

```tsx
const [nextPricing, nextStations, nextChanges] = await Promise.all([
  listPricingRules(),
  listStations(),
  listChangeEvents(),
]);
setChangeEvents(nextChanges);
```

Then load snapshots after `nextStations` as in Task 12.

- [ ] **Step 3: Show recent pricing changes in inspector**

Inside pricing inspector when `selected` exists, add:

```tsx
const selectedChanges = changeEvents.filter(
  (event) =>
    event.stationId === selected.stationId &&
    (event.eventType === "price_changed" || event.eventType === "rate_changed" || event.eventType === "model_added" || event.eventType === "model_removed"),
);
```

If this is inside render scope, compute before `return`.

Render:

```tsx
<div className="rounded-[var(--surface-radius)] border border-border bg-white p-3">
  <div className="text-xs font-semibold text-slate-700">最近价格 / 倍率变更</div>
  <div className="mt-2 grid gap-2">
    {selectedChanges.length === 0 ? (
      <div className="text-xs text-muted-foreground">暂无相关变更。</div>
    ) : (
      selectedChanges.slice(0, 5).map((event) => (
        <div key={event.id} className="text-xs leading-5 text-muted-foreground">
          <span className="font-medium text-slate-700">{event.title}</span>：{event.message}
        </div>
      ))
    )}
  </div>
</div>
```

- [ ] **Step 4: Run checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 5: Commit cross-page change links**

Run:

```powershell
git add -- src/features/stations/StationsPage.tsx src/features/pricing/PricingPage.tsx
git commit -m "feat: surface change events in asset details"
```

Expected:

- Commit succeeds.

---

## Task 15: Documentation Update

**Files:**
- Modify: `docs/PROJECT_PLAN.md`

- [ ] **Step 1: Update product positioning**

In `docs/PROJECT_PLAN.md`, update the one-sentence definition section to include:

```markdown
> Relay Pool Desktop 是一个本地 AI 中转资产管理与路由网关控制台：对外提供固定 OpenAI-compatible 入口，对内管理多个 Sub2API / NewAPI / OpenAI-compatible 中转站账号及其 Station Key，持续采集余额、倍率、价格和模型能力，并通过变更中心追踪风险变化，最终根据能力、健康、价格、余额和策略进行本地路由。
```

- [ ] **Step 2: Add page responsibility section**

Add section:

```markdown
## 信息架构

- 总览：回答“现在有什么风险？”，展示本地代理、未读风险、今日请求、失败率和成本摘要。
- 中转站资产：回答“哪个站点资产状态好不好？”，展示站点、类型、Base URL、余额、倍率摘要、采集状态、Key 数、健康、更新时间和路由参与状态。
- Key 池：回答“哪把 Key 能不能路由？”，管理 Station Key 的启用、优先级、能力、模型范围、健康和备用状态。
- 路由规则：回答“为什么请求会走这把 Key？”，管理默认策略、模型映射和路由模拟解释。
- 价格 / 倍率：回答“哪个站点更便宜？”，展示模型价格、分组倍率和模型可用性的跨站点矩阵。
- 渠道状态：回答“最近运行稳不稳？”，展示 Key / Channel 的成功率、延迟、冷却和最近请求状态。
- 变更中心：回答“最近有什么需要注意的变化？”，记录余额、Key、站点、采集、价格、倍率、模型和路由影响事件。
- 请求日志：回答“某次请求为什么成功或失败？”，展示请求、耗时、成本、fallback 和拒绝候选。
- 设置：回答“全局行为如何配置？”，管理本地代理、安全、数据目录、采集周期和阈值。
```

- [ ] **Step 3: Run docs diff review**

Run:

```powershell
git diff -- docs/PROJECT_PLAN.md
```

Expected:

- Diff only changes product positioning and IA docs.

- [ ] **Step 4: Commit docs**

Run:

```powershell
git add -- docs/PROJECT_PLAN.md
git commit -m "docs: document upgraded product information architecture"
```

Expected:

- Commit succeeds.

---

## Task 16: End-to-End Verification and Polish

**Files:**
- No planned edits unless verification reveals issues

- [ ] **Step 1: Run full frontend checks**

Run:

```powershell
pnpm.cmd tsc --noEmit
pnpm.cmd build
```

Expected:

- PASS.

- [ ] **Step 2: Run Rust checks**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml change_event --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected:

- PASS.

- [ ] **Step 3: Optional local app smoke**

If the repo's current state supports a dev server, run:

```powershell
pnpm.cmd dev
```

Then inspect these routes manually:

- 总览
- 中转站资产
- Key 池
- 路由规则
- 价格 / 倍率
- 渠道状态
- 变更中心
- 请求日志
- 设置

Expected:

- Sidebar labels match the new IA.
- `信息采集` is not first-level navigation.
- `变更中心` opens and shows mock or real events.
- Sidebar badge appears only when unread critical/warning events exist.
- 中转站资产 uses high-density table and right drawer.
- 价格 / 倍率 primary view is matrix-based.

- [ ] **Step 4: Review final git scope**

Run:

```powershell
git status --short -- .
git log --oneline -8
```

Expected:

- Working tree contains no accidental unrelated edits.
- Recent commits are concern-based.

- [ ] **Step 5: Final report**

Report:

- What changed.
- How to launch.
- How it was verified.
- Which event types are implemented.
- Which event types are deferred or partially implemented.
- Any existing user changes that were intentionally left untouched.

---

## Self-Review

### Spec Coverage

- Current page structure audit: covered in plan context and navigation tasks.
- New IA: Tasks 7, 8, 15.
- Page responsibilities: Tasks 7, 8, 10, 12, 15.
- 中转站资产 layout: Tasks 9, 10, 14.
- 价格 / 倍率 layout: Tasks 11, 12, 14.
- 变更中心 model and page: Tasks 2, 3, 4, 5, 6, 13.
- Rename/merge/keep/deprecate pages: Task 7 and Task 15.
- Phased implementation: Tasks 1-16.
- High-risk changes addressed: persistent event dedupe, staged diff logic, right drawer, matrix helpers, verification.
- Data model additions: `change_events`, frontend `ChangeEvent`, view model helpers.
- Confirmed user decisions: included in Confirmed Product Decisions.

### Placeholder Scan

The plan avoids placeholder markers, vague "handle edge cases" instructions, and unbounded "write tests" steps. Where exact current input names may differ, the plan gives the concrete intended code and requires adapting only to already-existing type names after reading the file.

### Type Consistency

- Rust `ChangeEvent` uses snake_case serde; TypeScript expects camelCase via Tauri serde conversion. If Tauri does not convert case automatically in this project, add `#[serde(rename_all = "camelCase")]` to `ChangeEvent` too before frontend integration.
- TypeScript `ChangeEvent.eventType` maps to Rust `event_type` through serde.
- Event statuses are consistently `unread`, `read`, `dismissed`, `resolved`.
- Severities are consistently `critical`, `warning`, `info`.
