# P6 Routing Policy and Model Capabilities Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade Relay Pool Desktop from a priority-fallback local gateway into a model-aware, protocol-aware, health-aware Station Key router with explainable decisions.

**Architecture:** P6 keeps `Station` as the account asset and routes only through `Station Key`. Add a small routing domain beside the existing proxy runtime: model aliases, key capabilities, key routing scopes, request-derived health, policy selection, and route explanation. Keep the first version deterministic and inspectable: no price optimization, no strategy DSL, no cloud sync, and no secret storage changes.

**Tech Stack:** Tauri 2, Rust, SQLite via existing `AppDatabase`, React + TypeScript + Vite, existing proxy runtime, existing Key Pool / request log models, OpenAI-compatible JSON, existing request-log-derived channel status.

---

## P6 Completion Definition

P6 is complete only when all of these are true:

1. Incoming proxy requests are routed through a dedicated selector, not by raw `enabled + priority` only.
2. The selector filters candidates by endpoint kind: `chat_completions`, `responses`, and optionally `embeddings` if implemented.
3. The selector filters candidates by requested model using explicit key scope plus model aliases.
4. The selector filters out disabled keys, cooldown keys, and keys with explicit capability mismatches.
5. The selector scores remaining keys according to a selected policy:
   - `priority_fallback`
   - `stable_first`
   - `backup_only`
6. Request logs update per-key health state: last success, last failure, consecutive failures, average latency, success rate, recent error, and cooldown expiration.
7. Fallback uses the same selector and health state; it does not keep hitting a key that is in cooldown.
8. Routing Rules page can simulate a request and explain candidate ordering and rejection reasons.
9. Key Pool displays capability summary, routing scope, health, and cooldown state per key.
10. Channel Status shows real key/channel health, not mock-only state.
11. Tests cover selector filtering, model aliasing, cooldown, scoring, fallback, health updates, and route explanation.
12. `pnpm build`, `cargo check --manifest-path .\src-tauri\Cargo.toml`, and `cargo test --manifest-path .\src-tauri\Cargo.toml --lib` pass.

P6 is not complete if the proxy still effectively chooses `enabled keys -> priority -> first success` without explaining model/protocol/health decisions.

## Explicit Non-Goals

Do not implement these in P6:

- Price-optimal routing.
- Balance-based automatic avoidance.
- Full cost calculation.
- Automatic model price collection.
- Complex strategy DSL.
- Team/shared/cloud configuration.
- Secret encryption migration.
- LAN exposure or auth for local management APIs.
- Full SSE event rewriting.
- Mid-stream fallback after bytes have been sent.
- Automatic perfect model capability discovery for every upstream.

P6 may store enough data to support future price/health features, but it must not build those policies yet.

## Existing Baseline

P5 already provides:

- Local proxy on `127.0.0.1`.
- `/v1/models` aggregation/deduplication.
- `/v1/chat/completions` non-streaming and SSE passthrough.
- `/v1/responses` non-streaming and SSE passthrough.
- Enabled Station Key routing by priority.
- Retryable fallback.
- `request_logs`.
- Key Pool and Channel Status reading real proxy metadata.

P6 must preserve all P5 behavior while making candidate choice smarter.

## File Map

### Rust Backend

- Modify: `src-tauri/src/models/proxy.rs`
  - Add route request/decision DTOs, policy enums, capability enums, simulator DTOs.
- Modify: `src-tauri/src/models/station_keys.rs`
  - Add key routing fields to `StationKey` and `KeyPoolItem`.
- Create: `src-tauri/src/models/routing.rs`
  - Shared serializable models for aliases, capabilities, key scopes, health snapshots, and route explanations.
- Modify: `src-tauri/src/models/mod.rs`
  - Export `routing`.
- Modify: `src-tauri/src/services/database.rs`
  - Add schema tables/columns, migrations, CRUD, and health update helpers.
- Modify: `src-tauri/src/services/proxy/mod.rs`
  - Keep generic helpers; expose route request kind helpers if useful.
- Modify: `src-tauri/src/services/proxy/router.rs`
  - Replace placeholder with selector/scoring/rejection logic.
- Modify: `src-tauri/src/services/proxy/runtime.rs`
  - Use router selector for models/chat/responses candidate ordering and fallback.
- Modify: `src-tauri/src/commands/mod.rs`
  - Add commands for capabilities, aliases, routing config, health snapshots, and simulation.

### Frontend

- Modify: `src/lib/types/proxy.ts`
  - Add route explanation and simulation types.
- Modify: `src/lib/types/stationKeys.ts`
  - Add capability/routing/health fields.
- Create: `src/lib/types/routing.ts`
  - Model alias, key capability, routing policy, simulation DTOs.
- Create: `src/lib/api/routing.ts`
  - Tauri invoke wrappers.
- Modify: `src/lib/api/stationKeys.ts`
  - Include routing/capability update payloads.
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
  - Show key capabilities/scope/health/cooldown and edit routing fields.
- Modify: `src/features/routing/RoutingPage.tsx`
  - Replace static mock page with default policy, aliases, and simulator.
- Modify: `src/features/channels/ChannelStatusPage.tsx`
  - Read key health snapshots instead of deriving only from logs.
- Modify: `src/features/logs/LogsPage.tsx`
  - Show selected key, policy, and high-level route reason if stored.
- Modify: `src/features/settings/SettingsPage.tsx`
  - Keep strategy setting aligned with new policy enum.

### Docs

- Modify: `README.md`
- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/PRODUCT_MODEL.md`
- Create or modify: `docs/PHASE_6_ROUTING_POLICY_PLAN.md`

---

## Data Model Target

P6 should add the smallest durable schema that can support routing decisions without overbuilding.

### New `station_key_capabilities` Table

One row per key. Defaults should be permissive enough not to break existing users after migration.

```sql
CREATE TABLE IF NOT EXISTS station_key_capabilities (
    station_key_id TEXT PRIMARY KEY,
    supports_chat_completions INTEGER NOT NULL DEFAULT 1,
    supports_responses INTEGER NOT NULL DEFAULT 1,
    supports_embeddings INTEGER NOT NULL DEFAULT 0,
    supports_stream INTEGER NOT NULL DEFAULT 1,
    supports_tools INTEGER NOT NULL DEFAULT 0,
    supports_vision INTEGER NOT NULL DEFAULT 0,
    supports_reasoning INTEGER NOT NULL DEFAULT 0,
    model_allowlist_json TEXT NOT NULL DEFAULT '[]',
    model_blocklist_json TEXT NOT NULL DEFAULT '[]',
    preferred_models_json TEXT NOT NULL DEFAULT '[]',
    only_use_as_backup INTEGER NOT NULL DEFAULT 0,
    routing_tags_json TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL,
    FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE
);
```

### New `model_aliases` Table

Global alias map. It maps the client-facing model name to the upstream model name. P6 can start global; station/key-specific aliases are later work.

```sql
CREATE TABLE IF NOT EXISTS model_aliases (
    id TEXT PRIMARY KEY,
    client_model TEXT NOT NULL,
    upstream_model TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_model_aliases_client_upstream
    ON model_aliases(client_model, upstream_model);
```

### New `station_key_health` Table

Derived from proxy traffic. Do not store prompt or response text.

```sql
CREATE TABLE IF NOT EXISTS station_key_health (
    station_key_id TEXT PRIMARY KEY,
    last_success_at TEXT,
    last_failure_at TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    avg_latency_ms INTEGER,
    last_error_summary TEXT,
    cooldown_until TEXT,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(station_key_id) REFERENCES station_keys(id) ON DELETE CASCADE
);
```

### Request Log Extension

Add optional route metadata to `request_logs`.

```sql
ALTER TABLE request_logs ADD COLUMN route_policy TEXT;
ALTER TABLE request_logs ADD COLUMN route_reason TEXT;
ALTER TABLE request_logs ADD COLUMN rejected_candidates_json TEXT;
```

If columns already exist, migration should skip safely.

### Policy Setting

Reuse existing `settings.default_routing_strategy`, but normalize values to:

```text
priority_fallback
stable_first
backup_only
```

Maintain compatibility with old values:

```text
manual -> priority_fallback
stable -> stable_first
cheapest -> priority_fallback for P6, because price routing is non-goal
```

---

## Task 1: Lock P6 Data Contracts and Migrations

**Files:**
- Modify: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/models/routing.rs`
- Modify: `src-tauri/src/models/proxy.rs`
- Modify: `src-tauri/src/models/station_keys.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add routing model types**

Create `src-tauri/src/models/routing.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoutingPolicy {
    PriorityFallback,
    StableFirst,
    BackupOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RouteEndpointKind {
    Models,
    ChatCompletions,
    Responses,
    Embeddings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyCapabilities {
    pub station_key_id: String,
    pub supports_chat_completions: bool,
    pub supports_responses: bool,
    pub supports_embeddings: bool,
    pub supports_stream: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub model_allowlist: Vec<String>,
    pub model_blocklist: Vec<String>,
    pub preferred_models: Vec<String>,
    pub only_use_as_backup: bool,
    pub routing_tags: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStationKeyCapabilitiesInput {
    pub station_key_id: String,
    pub supports_chat_completions: bool,
    pub supports_responses: bool,
    pub supports_embeddings: bool,
    pub supports_stream: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub model_allowlist: Vec<String>,
    pub model_blocklist: Vec<String>,
    pub preferred_models: Vec<String>,
    pub only_use_as_backup: bool,
    pub routing_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAlias {
    pub id: String,
    pub client_model: String,
    pub upstream_model: String,
    pub enabled: bool,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertModelAliasInput {
    pub id: Option<String>,
    pub client_model: String,
    pub upstream_model: String,
    pub enabled: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyHealth {
    pub station_key_id: String,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub consecutive_failures: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub avg_latency_ms: Option<i64>,
    pub last_error_summary: Option<String>,
    pub cooldown_until: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSimulationInput {
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub policy: Option<RoutingPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteCandidateExplanation {
    pub station_key_id: String,
    pub station_id: String,
    pub station_name: String,
    pub key_name: String,
    pub accepted: bool,
    pub score: i64,
    pub reasons: Vec<String>,
    pub rejection_reasons: Vec<String>,
    pub mapped_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSimulationResult {
    pub selected_station_key_id: Option<String>,
    pub selected_station_id: Option<String>,
    pub mapped_model: Option<String>,
    pub policy: RoutingPolicy,
    pub candidates: Vec<RouteCandidateExplanation>,
    pub message: String,
}
```

- [ ] **Step 2: Export routing models**

Modify `src-tauri/src/models/mod.rs`:

```rust
pub mod routing;
```

- [ ] **Step 3: Extend `KeyPoolItem` and `StationKey`**

Modify `src-tauri/src/models/station_keys.rs` to include only summary fields, not full JSON policy blobs:

```rust
pub capability_summary: Vec<String>,
pub model_scope_summary: String,
pub only_use_as_backup: bool,
pub cooldown_until: Option<String>,
pub success_rate: Option<f64>,
pub avg_latency_ms: Option<i64>,
pub consecutive_failures: i64,
pub last_error_summary: Option<String>,
```

Add those fields to both `StationKey` and `KeyPoolItem` only if the page needs them; otherwise add them to `KeyPoolItem` first to reduce churn.

- [ ] **Step 4: Write failing database migration test**

Add this test in `src-tauri/src/services/database.rs` test module. If no test module exists yet, create `#[cfg(test)] mod tests`.

```rust
#[test]
fn routing_tables_exist_in_new_database() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let connection = database.connection().expect("connection");

    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN (
                'station_key_capabilities',
                'model_aliases',
                'station_key_health'
            )",
            [],
            |row| row.get(0),
        )
        .expect("table count");

    assert_eq!(count, 3);
}
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml routing_tables_exist_in_new_database --lib
```

Expected: FAIL because tables do not exist yet.

- [ ] **Step 5: Implement schema and migrations**

Modify `initialize_schema` in `src-tauri/src/services/database.rs` to create the three new tables.

Add a migration helper:

```rust
fn migrate_request_log_route_columns(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(request_logs)")?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;

    if !rows.iter().any(|column| column == "route_policy") {
        connection.execute("ALTER TABLE request_logs ADD COLUMN route_policy TEXT", [])?;
    }
    if !rows.iter().any(|column| column == "route_reason") {
        connection.execute("ALTER TABLE request_logs ADD COLUMN route_reason TEXT", [])?;
    }
    if !rows.iter().any(|column| column == "rejected_candidates_json") {
        connection.execute("ALTER TABLE request_logs ADD COLUMN rejected_candidates_json TEXT", [])?;
    }
    Ok(())
}
```

Call it from `AppDatabase::initialize` and `new_in_memory_for_tests`.

- [ ] **Step 6: Run migration tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml routing_tables_exist_in_new_database --lib
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add -- src-tauri/src/models/mod.rs src-tauri/src/models/routing.rs src-tauri/src/models/proxy.rs src-tauri/src/models/station_keys.rs src-tauri/src/services/database.rs
git commit -m "feat: add routing capability data model"
```

**Task 1 Done When:**

- New routing/capability/health tables exist.
- Existing databases migrate safely.
- Existing P5 tests pass.
- No UI behavior changes yet.

---

## Task 2: Add Routing Data CRUD Commands

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Create: `src/lib/types/routing.ts`
- Create: `src/lib/api/routing.ts`

- [ ] **Step 1: Write failing Rust CRUD tests**

Add tests in `database.rs`:

```rust
#[test]
fn station_key_capabilities_round_trip() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "routing-capabilities");
    let key = database.list_station_keys(station.id.clone()).expect("keys").remove(0);

    let input = UpdateStationKeyCapabilitiesInput {
        station_key_id: key.id.clone(),
        supports_chat_completions: true,
        supports_responses: false,
        supports_embeddings: true,
        supports_stream: true,
        supports_tools: true,
        supports_vision: false,
        supports_reasoning: true,
        model_allowlist: vec!["gpt-4o-mini".to_string()],
        model_blocklist: vec!["gpt-4o".to_string()],
        preferred_models: vec!["gpt-4o-mini".to_string()],
        only_use_as_backup: false,
        routing_tags: vec!["cheap".to_string()],
    };

    let saved = database.update_station_key_capabilities(input).expect("save");
    let loaded = database.get_station_key_capabilities(key.id).expect("load");

    assert_eq!(loaded.station_key_id, saved.station_key_id);
    assert_eq!(loaded.model_allowlist, vec!["gpt-4o-mini"]);
    assert_eq!(loaded.model_blocklist, vec!["gpt-4o"]);
    assert!(loaded.supports_tools);
    assert!(loaded.supports_reasoning);
}

#[test]
fn model_alias_round_trip() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let saved = database
        .upsert_model_alias(UpsertModelAliasInput {
            id: None,
            client_model: "gpt-4o-mini".to_string(),
            upstream_model: "openai/gpt-4o-mini".to_string(),
            enabled: true,
            note: Some("test alias".to_string()),
        })
        .expect("save alias");

    let aliases = database.list_model_aliases().expect("aliases");

    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases[0].id, saved.id);
    assert_eq!(aliases[0].client_model, "gpt-4o-mini");
    assert_eq!(aliases[0].upstream_model, "openai/gpt-4o-mini");
}
```

If `test_station` helper does not exist, define it under `#[cfg(test)]` using `create_station`.

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml station_key_capabilities_round_trip model_alias_round_trip --lib
```

Expected: FAIL because methods do not exist.

- [ ] **Step 2: Implement database methods**

Add to `impl AppDatabase`:

```rust
pub fn get_station_key_capabilities(&self, station_key_id: String) -> Result<StationKeyCapabilities, String>
pub fn update_station_key_capabilities(&self, input: UpdateStationKeyCapabilitiesInput) -> Result<StationKeyCapabilities, String>
pub fn list_model_aliases(&self) -> Result<Vec<ModelAlias>, String>
pub fn upsert_model_alias(&self, input: UpsertModelAliasInput) -> Result<ModelAlias, String>
pub fn delete_model_alias(&self, id: String) -> Result<(), String>
pub fn list_station_key_health(&self) -> Result<Vec<StationKeyHealth>, String>
pub fn get_station_key_health(&self, station_key_id: String) -> Result<StationKeyHealth, String>
```

Implementation rules:

- JSON arrays must be serialized/deserialized with `serde_json`.
- Empty allowlist means "allow all models unless blocked".
- Missing capability row returns defaults and creates no write until user saves.
- Alias `client_model` and `upstream_model` must be trimmed and non-empty.
- Never return full API keys.

- [ ] **Step 3: Add Tauri commands**

Modify `src-tauri/src/commands/mod.rs`:

```rust
#[tauri::command]
pub fn get_station_key_capabilities(
    database: State<'_, AppDatabase>,
    station_key_id: String,
) -> Result<StationKeyCapabilities, String> {
    database.get_station_key_capabilities(station_key_id)
}

#[tauri::command]
pub fn update_station_key_capabilities(
    database: State<'_, AppDatabase>,
    input: UpdateStationKeyCapabilitiesInput,
) -> Result<StationKeyCapabilities, String> {
    database.update_station_key_capabilities(input)
}

#[tauri::command]
pub fn list_model_aliases(database: State<'_, AppDatabase>) -> Result<Vec<ModelAlias>, String> {
    database.list_model_aliases()
}

#[tauri::command]
pub fn upsert_model_alias(
    database: State<'_, AppDatabase>,
    input: UpsertModelAliasInput,
) -> Result<ModelAlias, String> {
    database.upsert_model_alias(input)
}

#[tauri::command]
pub fn delete_model_alias(database: State<'_, AppDatabase>, id: String) -> Result<(), String> {
    database.delete_model_alias(id)
}

#[tauri::command]
pub fn list_station_key_health(database: State<'_, AppDatabase>) -> Result<Vec<StationKeyHealth>, String> {
    database.list_station_key_health()
}
```

Also register commands in `src-tauri/src/lib.rs` if this project registers commands there.

- [ ] **Step 4: Add frontend types**

Create `src/lib/types/routing.ts`:

```ts
export type RoutingPolicy = "priority_fallback" | "stable_first" | "backup_only";
export type RouteEndpointKind = "models" | "chat_completions" | "responses" | "embeddings";

export type StationKeyCapabilities = {
  stationKeyId: string;
  supportsChatCompletions: boolean;
  supportsResponses: boolean;
  supportsEmbeddings: boolean;
  supportsStream: boolean;
  supportsTools: boolean;
  supportsVision: boolean;
  supportsReasoning: boolean;
  modelAllowlist: string[];
  modelBlocklist: string[];
  preferredModels: string[];
  onlyUseAsBackup: boolean;
  routingTags: string[];
  updatedAt: string;
};

export type UpdateStationKeyCapabilitiesInput = Omit<StationKeyCapabilities, "updatedAt">;

export type ModelAlias = {
  id: string;
  clientModel: string;
  upstreamModel: string;
  enabled: boolean;
  note: string | null;
  createdAt: string;
  updatedAt: string;
};

export type UpsertModelAliasInput = {
  id: string | null;
  clientModel: string;
  upstreamModel: string;
  enabled: boolean;
  note: string | null;
};

export type StationKeyHealth = {
  stationKeyId: string;
  lastSuccessAt: string | null;
  lastFailureAt: string | null;
  consecutiveFailures: number;
  successCount: number;
  failureCount: number;
  avgLatencyMs: number | null;
  lastErrorSummary: string | null;
  cooldownUntil: string | null;
  updatedAt: string;
};
```

- [ ] **Step 5: Add frontend API wrappers**

Create `src/lib/api/routing.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";
import type {
  ModelAlias,
  StationKeyCapabilities,
  StationKeyHealth,
  UpdateStationKeyCapabilitiesInput,
  UpsertModelAliasInput,
} from "@/lib/types/routing";

export function getStationKeyCapabilities(stationKeyId: string) {
  return invoke<StationKeyCapabilities>("get_station_key_capabilities", { stationKeyId });
}

export function updateStationKeyCapabilities(input: UpdateStationKeyCapabilitiesInput) {
  return invoke<StationKeyCapabilities>("update_station_key_capabilities", { input });
}

export function listModelAliases() {
  return invoke<ModelAlias[]>("list_model_aliases");
}

export function upsertModelAlias(input: UpsertModelAliasInput) {
  return invoke<ModelAlias>("upsert_model_alias", { input });
}

export function deleteModelAlias(id: string) {
  return invoke<void>("delete_model_alias", { id });
}

export function listStationKeyHealth() {
  return invoke<StationKeyHealth[]>("list_station_key_health");
}
```

- [ ] **Step 6: Run verification**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml station_key_capabilities_round_trip --lib
cargo test --manifest-path .\src-tauri\Cargo.toml model_alias_round_trip --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm build
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src/lib/types/routing.ts src/lib/api/routing.ts
git commit -m "feat: add routing capability commands"
```

**Task 2 Done When:**

- Capabilities and aliases persist.
- Frontend can call new commands.
- No proxy routing behavior has changed yet.

---

## Task 3: Build the Route Selector Core

**Files:**
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/proxy/mod.rs`
- Modify: `src-tauri/src/models/proxy.rs`
- Modify: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Define selector input and output**

In `src-tauri/src/services/proxy/router.rs`, add:

```rust
use crate::{
    models::routing::{RouteCandidateExplanation, RouteEndpointKind, RoutingPolicy, StationKeyCapabilities, StationKeyHealth},
    services::proxy::RouteCandidate,
};

#[derive(Debug, Clone)]
pub struct RouteRequest {
    pub endpoint: RouteEndpointKind,
    pub model: Option<String>,
    pub stream: bool,
    pub uses_tools: bool,
    pub uses_vision: bool,
    pub uses_reasoning: bool,
    pub policy: RoutingPolicy,
    pub now_ms: i64,
}

#[derive(Debug, Clone)]
pub struct RichRouteCandidate {
    pub candidate: RouteCandidate,
    pub station_name: String,
    pub key_name: String,
    pub capabilities: StationKeyCapabilities,
    pub health: Option<StationKeyHealth>,
}

#[derive(Debug, Clone)]
pub struct RouteSelection {
    pub accepted: Vec<RichRouteCandidate>,
    pub explanations: Vec<RouteCandidateExplanation>,
    pub mapped_model: Option<String>,
}
```

- [ ] **Step 2: Write failing selector tests**

Add tests in `router.rs`:

```rust
#[test]
fn selector_rejects_protocol_mismatch() {
    let request = route_request(RouteEndpointKind::Responses, Some("gpt-4o-mini"), true, RoutingPolicy::PriorityFallback);
    let candidates = vec![
        rich_candidate("chat-only", 0, capabilities(|c| {
            c.supports_responses = false;
            c.supports_chat_completions = true;
        })),
        rich_candidate("responses", 10, capabilities(|c| {
            c.supports_responses = true;
        })),
    ];

    let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

    assert_eq!(selected.accepted[0].candidate.station_key_id, "responses");
    assert!(selected.explanations.iter().any(|item| {
        item.station_key_id == "chat-only" && item.rejection_reasons.iter().any(|reason| reason.contains("does not support responses"))
    }));
}

#[test]
fn selector_applies_alias_and_allowlist() {
    let request = route_request(RouteEndpointKind::ChatCompletions, Some("gpt-4o-mini"), false, RoutingPolicy::PriorityFallback);
    let aliases = vec![("gpt-4o-mini".to_string(), "openai/gpt-4o-mini".to_string())];
    let candidates = vec![
        rich_candidate("blocked", 0, capabilities(|c| {
            c.model_allowlist = vec!["other-model".to_string()];
        })),
        rich_candidate("allowed", 10, capabilities(|c| {
            c.model_allowlist = vec!["openai/gpt-4o-mini".to_string()];
        })),
    ];

    let selected = select_route_candidates(&request, candidates, &aliases).expect("selection");

    assert_eq!(selected.mapped_model.as_deref(), Some("openai/gpt-4o-mini"));
    assert_eq!(selected.accepted[0].candidate.station_key_id, "allowed");
}

#[test]
fn selector_skips_cooldown_keys() {
    let request = route_request(RouteEndpointKind::ChatCompletions, Some("gpt-4o-mini"), false, RoutingPolicy::PriorityFallback);
    let candidates = vec![
        rich_candidate_with_health("cooldown", 0, capabilities(|_| {}), health(|h| {
            h.cooldown_until = Some("9999999999999".to_string());
        })),
        rich_candidate("ready", 10, capabilities(|_| {})),
    ];

    let selected = select_route_candidates(&request, candidates, &[]).expect("selection");

    assert_eq!(selected.accepted[0].candidate.station_key_id, "ready");
}
```

The helper names above must be implemented in the test module. Keep them local to tests.

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml selector_ --lib
```

Expected: FAIL because selector does not exist.

- [ ] **Step 3: Implement filtering**

Implement:

```rust
pub fn select_route_candidates(
    request: &RouteRequest,
    candidates: Vec<RichRouteCandidate>,
    aliases: &[(String, String)],
) -> Result<RouteSelection, String>
```

Filtering rules:

- Disabled keys should already be absent from `RouteCandidate`, but keep a reason if the rich data includes disabled in future.
- Endpoint mismatch rejects:
  - `responses` requires `supports_responses`.
  - `chat_completions` requires `supports_chat_completions`.
  - `embeddings` requires `supports_embeddings`.
- `stream=true` rejects keys with `supports_stream=false`.
- `uses_tools=true` rejects keys with `supports_tools=false`.
- `uses_vision=true` rejects keys with `supports_vision=false`.
- `uses_reasoning=true` rejects keys with `supports_reasoning=false`.
- Model matching:
  - Apply first enabled alias where `client_model == request.model`.
  - If allowlist is non-empty, mapped model must be in allowlist.
  - If blocklist contains client or mapped model, reject.
- Cooldown:
  - If `cooldown_until` parses as milliseconds and is greater than `now_ms`, reject.

- [ ] **Step 4: Implement scoring**

Use deterministic integer scores. Lower score wins.

```rust
fn candidate_score(
    request: &RouteRequest,
    candidate: &RichRouteCandidate,
    mapped_model: Option<&str>,
) -> i64
```

Rules:

- `priority_fallback`: score starts with `candidate.priority * 1000`.
- `stable_first`: score starts with priority, then subtract success signal and add latency/failure penalties:
  - `priority * 1000`
  - `+ consecutive_failures * 500`
  - `+ avg_latency_ms.unwrap_or(5000) / 10`
  - `- success_count.min(100) * 5`
- `backup_only`:
  - Non-backup keys score normally.
  - Backup-only keys get `+ 100_000`.
  - If every accepted key is backup-only, allow them and order by priority.
- Preferred model:
  - If `preferred_models` contains client or mapped model, subtract `250`.

Do not introduce randomization in P6.

- [ ] **Step 5: Run selector tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml selector_ --lib
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/proxy/router.rs src-tauri/src/services/proxy/mod.rs src-tauri/src/models/proxy.rs src-tauri/src/services/database.rs
git commit -m "feat: add model-aware route selector"
```

**Task 3 Done When:**

- Selector can reject protocol/model/capability/cooldown mismatches.
- Selector can explain accepted and rejected candidates.
- Selector is deterministic and unit-tested.
- Proxy runtime still uses old routing until Task 4.

---

## Task 4: Wire Selector Into Proxy Runtime

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/models/proxy.rs`

- [ ] **Step 1: Add database method for rich candidates**

Add:

```rust
pub fn proxy_rich_route_candidates(&self) -> Result<Vec<RichRouteCandidate>, String>
pub fn enabled_model_alias_pairs(&self) -> Result<Vec<(String, String)>, String>
```

The SQL must join:

- `station_keys`
- `stations`
- `station_key_capabilities`
- `station_key_health`

For missing capability/health rows, return defaults.

- [ ] **Step 2: Write failing runtime tests**

Add tests in `runtime.rs`:

```rust
#[test]
fn chat_request_skips_key_that_does_not_allow_model() {
    let upstream_a = test_upstream_that_panics_if_called();
    let upstream_b = test_upstream_chat_success("pong");
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let key_a = create_test_station_key(&database, "blocked-model", upstream_a.base_url());
    let key_b = create_test_station_key(&database, "allowed-model", upstream_b.base_url());

    database.update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
        station_key_id: key_a.id.clone(),
        model_allowlist: vec!["other-model".to_string()],
        ..default_capabilities_input(key_a.id.clone())
    }).expect("blocked capabilities");
    database.update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
        station_key_id: key_b.id.clone(),
        model_allowlist: vec!["gpt-5.4".to_string()],
        ..default_capabilities_input(key_b.id.clone())
    }).expect("allowed capabilities");

    let response = forward_chat_request(&proxy_context(database), &chat_request("gpt-5.4", false));

    assert_eq!(response.station_key_id.as_deref(), Some(key_b.id.as_str()));
    assert_eq!(response.status_code, 200);
}

#[test]
fn responses_request_skips_chat_only_key() {
    // Same shape: first key supports responses=false, second supports responses=true.
    // Expected selected key is second key.
}
```

Implement the full second test instead of leaving this comment in code.

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml skips_key --lib
```

Expected: FAIL because runtime does not use selector yet.

- [ ] **Step 3: Build RouteRequest from HTTP request**

In `runtime.rs`, create helpers:

```rust
fn route_request_for_chat(model: Option<String>, stream: bool, body: &Value, policy: RoutingPolicy) -> RouteRequest
fn route_request_for_responses(model: Option<String>, stream: bool, body: &Value, policy: RoutingPolicy) -> RouteRequest
fn route_request_for_models(policy: RoutingPolicy) -> RouteRequest
```

Capability detection:

- `uses_tools`: JSON has `tools` array or `tool_choice`.
- `uses_vision`: any message/input item includes `image_url`, `input_image`, or `type == "image"`.
- `uses_reasoning`: JSON has `reasoning`, `reasoning_effort`, or model name starts with `o`.

- [ ] **Step 4: Use selector for candidate ordering**

Replace:

```rust
enabled_candidates(...)
preferred_candidates(...)
```

with:

```rust
let rich_candidates = context.database.proxy_rich_route_candidates()?;
let aliases = context.database.enabled_model_alias_pairs()?;
let route = select_route_candidates(&route_request, rich_candidates, &aliases)?;
let candidates = route.accepted.into_iter().map(|item| item.candidate).collect::<Vec<_>>();
```

For no accepted candidates, return OpenAI-style `503`:

```json
{
  "error": {
    "message": "没有可用 Station Key 支持该请求：model=gpt-5.4 endpoint=responses stream=true",
    "type": "relay_pool_error",
    "code": "no_route_candidates"
  }
}
```

- [ ] **Step 5: Rewrite outbound model when alias applies**

If `mapped_model` differs from request model:

- For chat: replace `body.model`.
- For responses: replace `body.model`.
- Request log `model` should store the client model.
- Route reason should mention mapped model.
- Upstream receives mapped model.

Add test:

```rust
#[test]
fn alias_rewrites_upstream_model_but_logs_client_model() {
    // Client sends gpt-4o-mini.
    // Alias maps to openai/gpt-4o-mini.
    // Test upstream asserts received model is openai/gpt-4o-mini.
    // Response metadata still has model Some("gpt-4o-mini").
}
```

- [ ] **Step 6: Keep `/v1/models` behavior compatible**

`/v1/models` should continue to aggregate enabled keys, but P6 may use capabilities to avoid keys that cannot serve model listing if that flag is added later. For P6, do not add a new flag. Keep P5 behavior and tests.

- [ ] **Step 7: Run runtime tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml skips_key --lib
cargo test --manifest-path .\src-tauri\Cargo.toml alias_rewrites_upstream_model_but_logs_client_model --lib
cargo test --manifest-path .\src-tauri\Cargo.toml proxy --lib
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected: PASS.

- [ ] **Step 8: Commit**

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/router.rs src-tauri/src/models/proxy.rs
git commit -m "feat: route proxy requests by model and capabilities"
```

**Task 4 Done When:**

- Proxy runtime no longer blindly routes by priority.
- Unsupported model/protocol/capability keys are skipped before network calls.
- Aliases rewrite upstream model names.
- Existing P5 stream and fallback tests still pass.

---

## Task 5: Add Health State and Cooldown Updates

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`

- [ ] **Step 1: Write failing health update tests**

Add tests:

```rust
#[test]
fn successful_request_updates_key_health_success() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let key = create_test_station_key(&database, "success-key", "http://127.0.0.1:1");

    database.record_station_key_success(&key.id, 123, "1000").expect("success");
    let health = database.get_station_key_health(key.id).expect("health");

    assert_eq!(health.success_count, 1);
    assert_eq!(health.failure_count, 0);
    assert_eq!(health.consecutive_failures, 0);
    assert_eq!(health.avg_latency_ms, Some(123));
    assert_eq!(health.last_success_at.as_deref(), Some("1000"));
}

#[test]
fn repeated_failures_enter_cooldown() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let key = create_test_station_key(&database, "failure-key", "http://127.0.0.1:1");

    database.record_station_key_failure(&key.id, "timeout", "1000").expect("failure 1");
    database.record_station_key_failure(&key.id, "timeout", "2000").expect("failure 2");
    database.record_station_key_failure(&key.id, "timeout", "3000").expect("failure 3");
    let health = database.get_station_key_health(key.id).expect("health");

    assert_eq!(health.failure_count, 3);
    assert_eq!(health.consecutive_failures, 3);
    assert!(health.cooldown_until.is_some());
}
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml station_key_success repeated_failures_enter_cooldown --lib
```

Expected: FAIL because methods do not exist.

- [ ] **Step 2: Implement health update methods**

Add:

```rust
pub fn record_station_key_success(&self, station_key_id: &str, duration_ms: i64, now: &str) -> Result<(), String>
pub fn record_station_key_failure(&self, station_key_id: &str, error_summary: &str, now: &str) -> Result<(), String>
```

Cooldown rule for P6:

```text
consecutive_failures < 3: no cooldown
3 failures: cooldown 2 minutes
4 failures: cooldown 5 minutes
5+ failures: cooldown 15 minutes
```

Use milliseconds if existing service time values are milliseconds.

- [ ] **Step 3: Wire runtime to health updates**

In `runtime.rs`:

- On final selected key success:
  - call `record_station_key_success`.
- On retryable upstream status for a candidate:
  - call `record_station_key_failure`.
- On network error:
  - call `record_station_key_failure`.
- On request client error before upstream:
  - do not update key health because no key was used.
- On stream success:
  - record success when upstream stream response is selected.
  - P6 does not need to track mid-stream duration precisely.

- [ ] **Step 4: Add cooldown route test**

Add:

```rust
#[test]
fn runtime_skips_key_in_cooldown_and_uses_next_candidate() {
    // First key has cooldown_until in the future.
    // First key upstream panics if called.
    // Second key returns success.
    // Expected selected station_key_id is second key.
}
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml cooldown --lib
```

Expected: PASS after implementation.

- [ ] **Step 5: Run verification**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml health --lib
cargo test --manifest-path .\src-tauri\Cargo.toml cooldown --lib
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/router.rs
git commit -m "feat: add key health cooldown routing"
```

**Task 5 Done When:**

- Health is durable and derived from real proxy attempts.
- Cooldown affects route selection.
- Consecutive failures no longer hammer the first priority key forever.

---

## Task 6: Add Routing Simulator Backend

**Files:**
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/models/routing.rs`
- Create: `src/lib/api/routing.ts`
- Modify: `src/lib/types/routing.ts`

- [ ] **Step 1: Write failing simulator test**

Add:

```rust
#[test]
fn simulate_route_returns_selected_key_and_rejection_reasons() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let blocked = create_test_station_key(&database, "blocked", "http://127.0.0.1:1");
    let selected = create_test_station_key(&database, "selected", "http://127.0.0.1:2");

    database.update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
        station_key_id: blocked.id.clone(),
        model_allowlist: vec!["other-model".to_string()],
        ..default_capabilities_input(blocked.id.clone())
    }).expect("blocked caps");

    let result = database.simulate_route(RouteSimulationInput {
        endpoint: RouteEndpointKind::ChatCompletions,
        model: Some("gpt-5.4".to_string()),
        stream: false,
        uses_tools: false,
        uses_vision: false,
        uses_reasoning: false,
        policy: Some(RoutingPolicy::PriorityFallback),
    }).expect("simulate");

    assert_eq!(result.selected_station_key_id.as_deref(), Some(selected.id.as_str()));
    assert!(result.candidates.iter().any(|candidate| {
        candidate.station_key_id == blocked.id && !candidate.accepted
    }));
}
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml simulate_route_returns_selected_key_and_rejection_reasons --lib
```

Expected: FAIL because simulator method does not exist.

- [ ] **Step 2: Implement simulator method**

Add:

```rust
pub fn simulate_route(&self, input: RouteSimulationInput) -> Result<RouteSimulationResult, String>
```

The simulator must:

- Not call upstream.
- Use same selector as runtime.
- Use same aliases/capabilities/health data as runtime.
- Return selected key id and ordered candidate explanations.
- Return readable message if no candidates accepted.

- [ ] **Step 3: Add Tauri command**

```rust
#[tauri::command]
pub fn simulate_route(
    database: State<'_, AppDatabase>,
    input: RouteSimulationInput,
) -> Result<RouteSimulationResult, String> {
    database.simulate_route(input)
}
```

- [ ] **Step 4: Add frontend API**

Extend `src/lib/api/routing.ts`:

```ts
import type { RouteSimulationInput, RouteSimulationResult } from "@/lib/types/routing";

export function simulateRoute(input: RouteSimulationInput) {
  return invoke<RouteSimulationResult>("simulate_route", { input });
}
```

Extend `src/lib/types/routing.ts`:

```ts
export type RouteSimulationInput = {
  endpoint: RouteEndpointKind;
  model: string | null;
  stream: boolean;
  usesTools: boolean;
  usesVision: boolean;
  usesReasoning: boolean;
  policy: RoutingPolicy | null;
};

export type RouteCandidateExplanation = {
  stationKeyId: string;
  stationId: string;
  stationName: string;
  keyName: string;
  accepted: boolean;
  score: number;
  reasons: string[];
  rejectionReasons: string[];
  mappedModel: string | null;
};

export type RouteSimulationResult = {
  selectedStationKeyId: string | null;
  selectedStationId: string | null;
  mappedModel: string | null;
  policy: RoutingPolicy;
  candidates: RouteCandidateExplanation[];
  message: string;
};
```

- [ ] **Step 5: Run verification**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml simulate_route --lib
cargo check --manifest-path .\src-tauri\Cargo.toml
pnpm build
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/services/proxy/router.rs src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/models/routing.rs src/lib/api/routing.ts src/lib/types/routing.ts
git commit -m "feat: add route simulation command"
```

**Task 6 Done When:**

- Simulator uses real selector.
- Simulator makes no network calls.
- Output explains selected and rejected candidates.

---

## Task 7: Upgrade Key Pool UI for Routing Scope

**Files:**
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/lib/api/routing.ts`
- Modify: `src/lib/types/routing.ts`
- Modify: `src/lib/types/stationKeys.ts`

- [ ] **Step 1: Add route capability display**

Each Key row must display:

```text
协议: Chat / Responses / Stream / Tools / Vision / Reasoning / Embeddings
模型: allowlist count, blocklist count, or "全部模型"
健康: success rate, avg latency, consecutive failures
冷却: active until timestamp or "正常"
标签: routing tags
```

Display rules:

- Empty allowlist: `全部模型`.
- Non-empty allowlist: `允许 N 个模型`.
- Any blocklist: append `屏蔽 N 个`.
- Cooldown active: show warning badge `冷却中`.
- Backup-only: show badge `备用`.

- [ ] **Step 2: Add edit dialog fields**

In key edit dialog, add controls:

```text
协议能力:
  [x] Chat Completions
  [x] Responses
  [ ] Embeddings
  [x] Stream
  [ ] Tools
  [ ] Vision
  [ ] Reasoning

模型范围:
  Allowlist textarea, one model per line
  Blocklist textarea, one model per line
  Preferred models textarea, one model per line

路由:
  [ ] 仅作为备用 key
  Tags input, comma separated
```

Do not add model picker autocomplete in P6.

- [ ] **Step 3: Save capabilities separately from API key**

When user saves key edit dialog:

1. Call existing `updateStationKey`.
2. Call `updateStationKeyCapabilities`.
3. Refresh key pool list.

If capability save fails after key save:

- Show error.
- Refresh from backend.
- Do not retry automatically.

- [ ] **Step 4: Add UI smoke checklist**

Manual smoke:

```text
1. Open Key 池.
2. Edit a key.
3. Toggle Responses off.
4. Add allowlist model gpt-5.4.
5. Mark as backup-only.
6. Save.
7. Reopen dialog and confirm values persist.
8. Confirm row shows protocol/model/backup summary.
```

- [ ] **Step 5: Run verification**

```powershell
pnpm build
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src/features/key-pool/KeyPoolPage.tsx src/lib/api/routing.ts src/lib/types/routing.ts src/lib/types/stationKeys.ts
git commit -m "feat: add key routing scope controls"
```

**Task 7 Done When:**

- User can configure key model/protocol scope.
- Empty API key field still does not overwrite existing key.
- Key Pool clearly shows why a key is routeable or backup-only.

---

## Task 8: Upgrade Routing Rules UI and Simulator

**Files:**
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/lib/api/routing.ts`
- Modify: `src/lib/types/routing.ts`
- Modify: `src/lib/api/settings.ts`
- Modify: `src/lib/types/settings.ts`

- [ ] **Step 1: Replace static Routing page**

Routing page must show:

```text
Default policy:
  priority_fallback / stable_first / backup_only

Model aliases:
  client model -> upstream model
  enabled
  note

Route simulator:
  endpoint
  model
  stream
  tools / vision / reasoning toggles
  simulate button

Result:
  selected key
  mapped model
  ordered accepted candidates
  rejected candidates with reasons
```

- [ ] **Step 2: Add alias CRUD UI**

MVP UI can be inline rows:

- Add alias button.
- Edit client model.
- Edit upstream model.
- Enable/disable.
- Delete with confirm.

No bulk import in P6.

- [ ] **Step 3: Add simulator form**

Default values:

```ts
{
  endpoint: "responses",
  model: "gpt-5.4",
  stream: true,
  usesTools: false,
  usesVision: false,
  usesReasoning: false,
  policy: null
}
```

Call `simulateRoute`.

- [ ] **Step 4: Display explanation**

Accepted row:

```text
1. Pro Key · 测试中转站
score 0
原因: supports responses, supports stream, model allowed, priority 0
```

Rejected row:

```text
Low Cost Key · rejected
原因: model gpt-5.4 is not in allowlist
```

- [ ] **Step 5: Save default policy**

Reuse existing settings update if `defaultRoutingStrategy` can be mapped to new enum. If the current type only supports old values, update TypeScript/Rust settings types to:

```ts
export type RoutingStrategy = "priority_fallback" | "stable_first" | "backup_only";
```

Backend must parse old values into new enum without breaking existing settings.

- [ ] **Step 6: Run UI verification**

```powershell
pnpm build
```

Manual smoke:

```text
1. Open 路由规则.
2. Create alias gpt-5.4 -> openai/gpt-5.4.
3. Run simulator for responses + stream.
4. Confirm selected key and rejection reasons are readable.
5. Change default policy to stable_first.
6. Refresh page and confirm it persists.
```

- [ ] **Step 7: Commit**

```powershell
git add -- src/features/routing/RoutingPage.tsx src/lib/api/routing.ts src/lib/types/routing.ts src/lib/api/settings.ts src/lib/types/settings.ts src-tauri/src/models/settings.rs src-tauri/src/services/database.rs
git commit -m "feat: add routing policy simulator"
```

**Task 8 Done When:**

- Routing Rules page is the main P6 control surface.
- User can manage aliases.
- User can simulate and understand route decisions.
- Default policy persists.

---

## Task 9: Upgrade Channel Status to Key Health

**Files:**
- Modify: `src/features/channels/ChannelStatusPage.tsx`
- Modify: `src/lib/api/routing.ts`
- Modify: `src/lib/types/routing.ts`

- [ ] **Step 1: Fetch key health snapshots**

Use:

```ts
const [keys, logs, health] = await Promise.all([
  listKeyPoolItems(),
  listRequestLogs(),
  listStationKeyHealth(),
]);
```

- [ ] **Step 2: Display durable health**

Each card must show:

- Key name.
- Station name.
- Enabled/disabled.
- Success count.
- Failure count.
- Success rate.
- Average latency.
- Consecutive failures.
- Cooldown state.
- Last error summary.
- Recent 60 request bars from logs.

- [ ] **Step 3: Make cooldown visually obvious**

If `cooldownUntil` is in the future:

- Badge: `冷却中`.
- Tone: warning.
- Show `冷却至 HH:mm:ss`.

- [ ] **Step 4: Run verification**

```powershell
pnpm build
```

Manual smoke:

```text
1. Trigger a successful local proxy request.
2. Open 渠道状态.
3. Confirm success count or last success is visible.
4. Trigger repeated failure against bad upstream.
5. Confirm consecutive failure and cooldown appear.
```

- [ ] **Step 5: Commit**

```powershell
git add -- src/features/channels/ChannelStatusPage.tsx src/lib/api/routing.ts src/lib/types/routing.ts
git commit -m "feat: show key health in channel status"
```

**Task 9 Done When:**

- Channel Status uses durable health plus logs.
- It is no longer only a transient log-derived view.
- Cooldown is visible to users.

---

## Task 10: Add Route Metadata to Logs

**Files:**
- Modify: `src-tauri/src/models/proxy.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/lib/types/proxy.ts`

- [ ] **Step 1: Extend log types**

Add optional fields:

```rust
pub route_policy: Option<String>,
pub route_reason: Option<String>,
pub rejected_candidates_json: Option<String>,
```

TypeScript:

```ts
routePolicy: string | null;
routeReason: string | null;
rejectedCandidatesJson: string | null;
```

- [ ] **Step 2: Write failing log metadata test**

```rust
#[test]
fn request_log_records_route_policy_and_reason_without_prompt() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let log = database.insert_request_log(CreateRequestLogInput {
        method: "POST".to_string(),
        path: "/v1/chat/completions".to_string(),
        model: Some("gpt-5.4".to_string()),
        stream: false,
        status: "success".to_string(),
        station_key_id: Some("key-1".to_string()),
        station_id: Some("station-1".to_string()),
        upstream_base_url: Some("https://example.test".to_string()),
        fallback_count: 0,
        error_message: None,
        route_policy: Some("priority_fallback".to_string()),
        route_reason: Some("selected key-1 because model allowed".to_string()),
        rejected_candidates_json: Some("[]".to_string()),
        started_at: "1000".to_string(),
        finished_at: Some("1100".to_string()),
        duration_ms: Some(100),
    }).expect("insert log");

    assert_eq!(log.route_policy.as_deref(), Some("priority_fallback"));
    assert!(!serde_json::to_string(&log).unwrap().contains("prompt"));
}
```

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml request_log_records_route_policy --lib
```

Expected: FAIL until types and SQL update.

- [ ] **Step 3: Write route metadata from runtime**

When selector returns explanations:

- `route_policy`: selected policy.
- `route_reason`: first accepted candidate reason summary.
- `rejected_candidates_json`: redacted candidate ids and rejection reasons only.

Do not store full request body or API keys.

- [ ] **Step 4: Update Logs UI**

Inspector should show:

- Policy.
- Selected reason.
- Rejected candidates count.
- Expandable rejected reasons.

Keep raw JSON hidden or absent.

- [ ] **Step 5: Run verification**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml request_log_records_route_policy --lib
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
pnpm build
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- src-tauri/src/models/proxy.rs src-tauri/src/services/database.rs src-tauri/src/services/proxy/runtime.rs src/features/logs/LogsPage.tsx src/lib/types/proxy.ts
git commit -m "feat: record route explanations in logs"
```

**Task 10 Done When:**

- Request logs explain route decisions.
- Logs still do not contain prompt/response/key material.

---

## Task 11: Final Docs and Product Copy

**Files:**
- Modify: `README.md`
- Modify: `docs/PROJECT_PLAN.md`
- Modify: `docs/PRODUCT_MODEL.md`
- Create: `docs/PHASE_6_ROUTING_POLICY_PLAN.md`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/routing/RoutingPage.tsx`
- Modify: `src/features/channels/ChannelStatusPage.tsx`

- [ ] **Step 1: Add phase document**

Create `docs/PHASE_6_ROUTING_POLICY_PLAN.md` with:

- P6 goal.
- P6 completed capabilities.
- Data model.
- Routing policies.
- Health/cooldown.
- Simulator behavior.
- Non-goals.
- Manual smoke checklist.
- Known limitations.

- [ ] **Step 2: Update README current status**

README should say:

```text
P6 adds model-aware, protocol-aware, health-aware Station Key routing with aliases, key capability scope, cooldown, route simulation, and route explanations.
```

- [ ] **Step 3: Update PROJECT_PLAN**

Ensure:

- Station is still account asset.
- Station Key is still route object.
- P6 does not include price optimization.
- P7/P8 can own pricing/balance.

- [ ] **Step 4: Search old misleading language**

Run:

```powershell
rg -n "按 Key 池顺序|盲选|候选站点|价格最优|stream:true 不支持|mock" README.md docs src
```

Expected:

- Historical docs may mention old phases only if clearly historical.
- Current P6/P5 docs must not describe current routing as blind priority only.

- [ ] **Step 5: Run verification**

```powershell
pnpm build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add -- README.md docs/PROJECT_PLAN.md docs/PRODUCT_MODEL.md docs/PHASE_6_ROUTING_POLICY_PLAN.md src/features/key-pool/KeyPoolPage.tsx src/features/routing/RoutingPage.tsx src/features/channels/ChannelStatusPage.tsx
git commit -m "docs: record p6 routing policy completion"
```

**Task 11 Done When:**

- Docs match behavior.
- UI copy consistently says router selects Station Keys, not Stations.
- P6 boundaries are explicit.

---

## Task 12: Final P6 Acceptance Gate

**Files:**
- No new files unless fixes are required.

- [ ] **Step 1: Run full automated checks**

```powershell
pnpm build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
git status --short
git log --oneline -12
```

Expected:

- Frontend build passes.
- Rust check passes.
- Rust lib tests pass.
- Git status contains no accidental database/log/key/screenshot files.

- [ ] **Step 2: Manual smoke - route selector**

Set up three keys:

```text
Key A:
  priority 0
  supports responses = false

Key B:
  priority 10
  supports responses = true
  allowlist = gpt-5.4

Key C:
  priority 20
  supports responses = true
  only backup = true
```

Run simulator:

```text
endpoint = responses
model = gpt-5.4
stream = true
```

Expected:

```text
selected = Key B
Key A rejected: does not support responses
Key C accepted but ordered after Key B because backup-only
```

- [ ] **Step 3: Manual smoke - real proxy**

Use a test key and run:

```powershell
$BASE = "http://127.0.0.1:<port>"
curl.exe "$BASE/v1/models"

$body = @'
{
  "model": "gpt-5.4",
  "messages": [{ "role": "user", "content": "reply pong" }],
  "stream": false
}
'@

curl.exe "$BASE/v1/chat/completions" -H "Content-Type: application/json" -d $body
```

Expected:

- Request succeeds through a key that supports model/protocol.
- Logs show selected key and route reason.
- No prompt or full key is stored in logs.

- [ ] **Step 4: Manual smoke - cooldown**

Create first key with bad upstream:

```text
Base URL: http://127.0.0.1:9
priority: 0
```

Send repeated requests until failure threshold.

Expected:

- Key enters cooldown.
- Simulator skips it.
- Real proxy skips it.
- Channel Status shows cooldown.

- [ ] **Step 5: Manual smoke - alias**

Create alias:

```text
client: gpt-5.4
upstream: openai/gpt-5.4
```

Use upstream test server or a station that expects mapped model.

Expected:

- Client still sends `gpt-5.4`.
- Upstream receives `openai/gpt-5.4`.
- Logs show client model and route reason mentions mapping.

- [ ] **Step 6: Sensitive data audit**

Run:

```powershell
git status --short
git diff --cached --name-status
rg -n "sk-[A-Za-z0-9]|Bearer [A-Za-z0-9]|cookie|session|password|token" README.md docs src src-tauri
```

Expected:

- No real API keys.
- No cookies/sessions/tokens.
- No local database/log/screenshot staged.

- [ ] **Step 7: Final commit if needed**

If smoke fixes were required:

```powershell
git add -- <exact changed paths>
git commit -m "fix: finalize p6 routing acceptance"
```

Do not push unless the user explicitly asks.

**P6 Final Done When:**

- All automated checks pass.
- Simulator and real proxy demonstrate model/protocol/health-aware routing.
- Cooldown changes routing behavior.
- Logs explain route decisions.
- UI shows routing scope and health.
- No sensitive files or secrets are staged.

---

## P6 Risk Register

| Risk | Mitigation |
|---|---|
| Model support data may be wrong | Make user-configured allow/block lists explicit and visible; simulator shows why a key matches. |
| Alias can route to wrong upstream model | Keep aliases user-editable, globally visible, and reflected in route explanation. |
| Health state can punish a temporary outage too strongly | Use simple cooldown windows and reset consecutive failures on success. |
| Stable-first policy may be opaque | Show scores and reasons in simulator. |
| UI can become too complex | Keep Key Pool controls compact; put reasoning in Routing page simulator. |
| Request logs may leak sensitive data | Store only metadata, route reasons, and rejected ids/reasons. Never store prompt/response/body/key. |
| Existing users may be broken by strict defaults | Default capabilities should allow chat/responses/stream and all models. Users opt into restrictions. |
| P6 grows into price routing | Keep price/balance policy explicitly out of P6. |

## P6 Commit Plan

Use concern-based commits:

1. `feat: add routing capability data model`
2. `feat: add routing capability commands`
3. `feat: add model-aware route selector`
4. `feat: route proxy requests by model and capabilities`
5. `feat: add key health cooldown routing`
6. `feat: add route simulation command`
7. `feat: add key routing scope controls`
8. `feat: add routing policy simulator`
9. `feat: show key health in channel status`
10. `feat: record route explanations in logs`
11. `docs: record p6 routing policy completion`

Do not use `git add .`. Stage exact files per task.

## P6 Final Summary Template

When P6 is complete, report:

```text
P6 completed:
- model/protocol capability data model
- model alias mapping
- key allowlist/blocklist/preferred models
- priority/stable/backup routing policies
- health state and cooldown
- proxy runtime selector integration
- route simulator
- Key Pool routing controls
- Channel Status key health
- route explanations in logs

Verification:
- pnpm build: pass
- cargo check: pass
- cargo test --lib: pass
- manual simulator smoke: pass
- manual proxy smoke: pass
- sensitive data audit: pass

Known non-goals left for P7/P8:
- price routing
- balance avoidance
- cost calculation
- strategy DSL
- secret encryption migration
```
