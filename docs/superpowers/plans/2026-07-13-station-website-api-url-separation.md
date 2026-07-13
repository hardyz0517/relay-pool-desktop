# Station Website and API URL Separation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the ambiguous Station Base URL with required website and API namespace URLs across persistence, collection, authorization, monitoring, routing, and UI without allowing stale work or credentials to cross endpoint revisions.

**Architecture:** Add a focused Rust `station_endpoints` module for structured URL validation, namespace construction, origin comparison, and allowlist matching. Persist `website_url`, `api_base_url`, and `endpoint_revision` on Station, propagate the explicit roles through existing models and projections, and use revision-checked writes for background results. Keep management and upstream traffic on separate call paths; keep existing Station, Station Key, collector, and routing ownership boundaries.

**Tech Stack:** Tauri 2, Rust 2021, rusqlite 0.32 with bundled SQLite, ureq 2, React 18, TypeScript 5.7, Vite 6, TanStack Query, Tailwind CSS, Node test scripts.

---

## Execution Notes

- Read `docs/superpowers/specs/2026-07-13-station-website-api-url-separation-design.md` before starting.
- The worktree may contain user changes in `src-tauri/src/services/capture/web_authorization.rs` and related authorization files. Preserve and extend them; never replace or revert them.
- Do not use `git add .` or `git add -A`. Every commit command below stages explicit paths only.
- Run `git status --short` before each task. If a listed file contains unrelated user edits, keep them and apply the task around them.
- The API Base URL is a complete namespace before the resource path. `https://x/v1` plus `/v1/responses` becomes `https://x/v1/responses`; `https://x/api/v3` plus `/v1/responses` becomes `https://x/api/v3/responses`.

## File Structure

### New files

- `src-tauri/src/services/station_endpoints.rs`: structured endpoint validation, normalization, migration derivation, origin comparison, safe base membership, and API resource URL construction.
- `scripts/station-endpoint-contract.test.mjs`: frontend source/data contract for dual URLs, explicit presets, and removal of ambiguous Station fields.
- `scripts/station-endpoint-ui.test.mjs`: UI source contract for both forms, list/detail labels, browser links, and read-only Key context.

### Backend files modified

- `src-tauri/Cargo.toml`: add direct `url` dependency.
- `src-tauri/src/services/mod.rs`: export `station_endpoints`.
- `src-tauri/src/models/stations.rs`: Station URL fields and endpoint revision.
- `src-tauri/src/models/station_keys.rs`: `station_api_base_url` and `station_endpoint_revision` projection fields.
- `src-tauri/src/models/collector.rs`: revision-tagged snapshots and website-specific login input.
- `src-tauri/src/models/collector_runs.rs`: endpoint revision and `superseded` status.
- `src-tauri/src/services/database.rs`: fresh schema, idempotent migration, row mapping, atomic endpoint update safeguards, revision-aware persistence, and API projection queries.
- `src-tauri/src/services/collectors/url.rs`: retain only management joining or remove after all callers move to `station_endpoints`.
- `src-tauri/src/services/collectors/mod.rs`: explicit endpoint roles and revision capture.
- `src-tauri/src/services/collectors/apply.rs`: atomic revision-checked snapshot/fact/run application.
- `src-tauri/src/services/collectors/sub2api.rs`: website login/management URL use.
- `src-tauri/src/services/collectors/adapters/openai_compatible.rs`: API namespace model URL use.
- `src-tauri/src/services/collectors/adapters/sub2api.rs`: classify each management or upstream endpoint explicitly.
- `src-tauri/src/services/collectors/adapters/newapi/auth.rs`: website login URL.
- `src-tauri/src/services/collectors/adapters/newapi/client.rs`: website management client.
- `src-tauri/src/services/collectors/adapters/newapi/mod.rs`: website status/management endpoints.
- `src-tauri/src/services/remote_keys.rs`: website URL for remote key CRUD.
- `src-tauri/src/services/capture/session.rs`: endpoint revision captured with authorization sessions.
- `src-tauri/src/services/capture/web_authorization.rs`: website verification and revision-aware persistence; merge with existing user changes.
- `src-tauri/src/services/outbound.rs`: zero-redirect credential-bearing agent helper.
- `src-tauri/src/services/endpoint_ping.rs`: API-derived probe target.
- `src-tauri/src/services/channel_monitors/mod.rs`: API projection and revision-checked health writes.
- `src-tauri/src/services/channel_monitors/probe.rs`: shared API namespace builder and redirect-safe agent.
- `src-tauri/src/services/proxy/mod.rs`: delegate upstream URL construction to `station_endpoints`.
- `src-tauri/src/services/proxy/runtime.rs`: API candidates, current revision feedback, and redirect behavior.
- `src-tauri/src/services/proxy/router.rs`: preserve endpoint revision through route selection.
- `src-tauri/src/commands/mod.rs`: website authorization, dual-origin capture checks, API PING/connectivity, and explicit command inputs.

### Frontend files modified

- `src/lib/types/stations.ts`, `src/lib/types/stationKeys.ts`, `src/lib/types/collector.ts`: explicit URL and revision contracts.
- `src/lib/api/stations.ts`, `src/lib/api/stationKeys.ts`, `src/lib/api/collector.ts`: Tauri calls and memory fallback.
- `src/lib/mock/stations.ts`: dual endpoint fixture data.
- `src/features/stations/providerPresets.ts`: explicit website and API values.
- `src/features/stations/AddProviderPage.tsx`, `src/features/stations/StationsPage.tsx`: both required fields, validation, copy action, and origin-change warnings.
- `src/features/stations/components/StationDetailContent.tsx`, `src/features/stations/components/StationDetailPanel.tsx`: distinct labels.
- `src/features/key-pool/AddKeyPage.tsx`, `src/features/key-pool/EditKeyPage.tsx`, `src/features/key-pool/KeyPoolPage.tsx`: read-only/displayed API URL.
- `src/features/dashboard/DashboardPage.tsx`: API route target summaries.
- `src/features/pricing/PricingPage.tsx`: website browser links.
- `src/features/collectors/CollectorsPage.tsx`: website descriptions.
- `src/lib/projections/runtimeSnapshot.ts`: API namespace and endpoint revision.
- `scripts/group-category-contract.test.mjs`, `scripts/pricing-facts-projection.test.mjs`, `scripts/pricing-group-comparison-view-model.test.mjs`, `scripts/pricing-station-browser-link.test.mjs`, `scripts/runtime-snapshot-projection.test.mjs`, `scripts/station-assets-current-projections.test.mjs`, `scripts/station-current-balance-projection.test.mjs`, `scripts/station-list-risk-tags.test.mjs`, and `scripts/station-url-browser-link.test.mjs`: explicit Station endpoint fixtures.

### Documentation modified

- `docs/PROJECT_PLAN.md`: Station definition and information architecture terminology.
- `README.md`: user-facing Station configuration terminology.

## Task 1: Add Structured Station Endpoint Semantics

**Files:**
- Create: `src-tauri/src/services/station_endpoints.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Test: `src-tauri/src/services/station_endpoints.rs`

- [ ] **Step 1: Write failing endpoint normalization and construction tests**

Add a `#[cfg(test)]` module covering the accepted contract:

```rust
#[test]
fn builds_resources_from_complete_api_namespaces() {
    assert_eq!(
        build_api_url("https://relay.example/v1", "/v1/responses").unwrap(),
        "https://relay.example/v1/responses"
    );
    assert_eq!(
        build_api_url("https://ark.example/api/v3", "/v1/chat/completions").unwrap(),
        "https://ark.example/api/v3/chat/completions"
    );
    assert_eq!(
        build_api_url("https://relay.example/proxy/v1", "/v1/models").unwrap(),
        "https://relay.example/proxy/v1/models"
    );
}

#[test]
fn rejects_final_resource_urls_as_api_bases() {
    let error = normalize_station_endpoints(
        "https://relay.example",
        "https://api.example/v1/responses",
    )
    .expect_err("final response URL must not be accepted as a base");
    assert!(error.contains("API Base URL"));
}

#[test]
fn derives_legacy_versioned_namespaces_without_corrupting_provider_paths() {
    assert_eq!(legacy_api_base_url("https://relay.example").unwrap(), "https://relay.example/v1");
    assert_eq!(legacy_api_base_url("https://relay.example/proxy").unwrap(), "https://relay.example/proxy/v1");
    assert_eq!(legacy_api_base_url("https://ark.example/api/v3").unwrap(), "https://ark.example/api/v3");
    assert_eq!(legacy_website_url("https://relay.example/v1").unwrap(), "https://relay.example");
}

#[test]
fn base_membership_uses_origin_and_path_boundaries() {
    assert!(url_belongs_to_base(
        "https://relay.example/api/user/self",
        "https://relay.example/api",
    ));
    assert!(!url_belongs_to_base(
        "https://relay.example.evil.test/api/user/self",
        "https://relay.example",
    ));
    assert!(!url_belongs_to_base(
        "https://relay.example/apix/user/self",
        "https://relay.example/api",
    ));
}
```

- [ ] **Step 2: Run the focused Rust test and confirm the module is missing**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml station_endpoints -- --nocapture
```

Expected: FAIL because `station_endpoints` and its functions do not exist.

- [ ] **Step 3: Add the direct URL dependency and module export**

Add to `src-tauri/Cargo.toml`:

```toml
url = "2"
```

Add to `src-tauri/src/services/mod.rs`:

```rust
pub mod station_endpoints;
```

- [ ] **Step 4: Implement the endpoint value functions**

Implement these public contracts in `station_endpoints.rs`:

```rust
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StationEndpointUrls {
    pub website_url: String,
    pub api_base_url: String,
}

pub fn normalize_station_endpoints(
    website_url: &str,
    api_base_url: &str,
) -> Result<StationEndpointUrls, String> {
    Ok(StationEndpointUrls {
        website_url: normalize_endpoint_url(website_url, "前端网址", false)?,
        api_base_url: normalize_endpoint_url(api_base_url, "API Base URL", true)?,
    })
}

pub fn build_management_url(base: &str, path: &str) -> Result<String, String> {
    append_resource(base, path.trim_start_matches('/'))
}

pub fn build_api_url(base: &str, local_path: &str) -> Result<String, String> {
    let resource = local_path
        .strip_prefix("/v1/")
        .or_else(|| local_path.strip_prefix("v1/"))
        .unwrap_or_else(|| local_path.trim_start_matches('/'));
    if resource.is_empty() || resource.contains("://") || resource.split('/').any(|part| part == "." || part == "..") {
        return Err("上游 API 资源路径无效".to_string());
    }
    append_resource(base, resource)
}

pub fn same_origin(left: &str, right: &str) -> Result<bool, String> {
    let left = Url::parse(left).map_err(|error| format!("URL 无效: {error}"))?;
    let right = Url::parse(right).map_err(|error| format!("URL 无效: {error}"))?;
    Ok(left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default())
}
```

Keep `normalize_endpoint_url`, `append_resource`, version-segment detection, final-resource rejection, `legacy_api_base_url`, `legacy_website_url`, and `url_belongs_to_base` private or `pub(crate)` according to their callers. Use parsed path segments; do not compare hosts with string prefixes.

- [ ] **Step 5: Run the endpoint tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml station_endpoints -- --nocapture
```

Expected: PASS for normalization, legacy derivation, namespace construction, origin comparison, and path-boundary membership.

- [ ] **Step 6: Commit the endpoint semantics**

```powershell
git add -- src-tauri/Cargo.toml src-tauri/src/services/mod.rs src-tauri/src/services/station_endpoints.rs
git commit -m "feat: define station endpoint URL semantics"
```

## Task 2: Migrate the Database and Rust Domain Contracts

**Files:**
- Modify: `src-tauri/src/models/stations.rs`
- Modify: `src-tauri/src/models/station_keys.rs`
- Modify: `src-tauri/src/models/collector.rs`
- Modify: `src-tauri/src/models/collector_runs.rs`
- Modify: `src-tauri/src/services/database.rs:146-220,1804-1814,6310-6660,7670-7780,8420-8590,11705-12680`
- Modify: `src-tauri/src/services/channel_monitors/mod.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/collectors/sub2api.rs`
- Modify: `src-tauri/src/services/collectors/apply.rs`
- Modify: `src-tauri/src/services/collectors/adapters/openai_compatible.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/auth.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/client.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`
- Modify: `src-tauri/src/services/collectors/adapters/sub2api.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/proxy/routing_snapshot.rs`
- Modify: `src-tauri/src/services/remote_keys.rs`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add failing migration and round-trip tests**

Add database tests with explicit legacy schemas:

```rust
#[test]
fn station_endpoint_migration_separates_legacy_root_and_versioned_urls() {
    let connection = legacy_station_connection(&[
        ("root", "https://root.example"),
        ("v1", "https://v1.example/v1"),
        ("v3", "https://ark.example/api/v3"),
    ]);

    migrate_station_endpoint_urls(&connection).expect("migrate endpoints");

    assert_station_urls(&connection, "root", "https://root.example", "https://root.example/v1", 1);
    assert_station_urls(&connection, "v1", "https://v1.example", "https://v1.example/v1", 1);
    assert_station_urls(&connection, "v3", "https://ark.example/api/v3", "https://ark.example/api/v3", 1);
    assert!(!station_columns(&connection).contains(&"base_url".to_string()));
    assert!(!station_columns(&connection).contains(&"upstream_api_base_path".to_string()));
}

#[test]
fn station_endpoint_migration_is_idempotent_and_fails_on_conflicting_columns() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let connection = database.connection().expect("connection");
    migrate_station_endpoint_urls(&connection).expect("first pass");
    migrate_station_endpoint_urls(&connection).expect("second pass");
    assert_eq!(station_columns(&connection).iter().filter(|name| *name == "api_base_url").count(), 1);
}

#[test]
fn station_create_round_trips_both_urls_and_revision() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = database.create_station(CreateStationInput {
        name: "Split Relay".to_string(),
        station_type: "newapi".to_string(),
        website_url: "https://console.example".to_string(),
        api_base_url: "https://api.example/v1".to_string(),
        api_key: String::new(),
        collector_proxy_mode: "inherit".to_string(),
        collector_proxy_url: None,
        enabled: true,
        credit_per_cny: 1.0,
        low_balance_threshold_cny: None,
        collection_interval_minutes: 5,
        note: None,
    }).expect("create station");
    assert_eq!(station.website_url, "https://console.example");
    assert_eq!(station.api_base_url, "https://api.example/v1");
    assert_eq!(station.endpoint_revision, 1);
}
```

- [ ] **Step 2: Run the focused database tests and confirm failure**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml station_endpoint_migration -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml station_create_round_trips_both_urls -- --nocapture
```

Expected: FAIL because the new columns, migration, and model fields do not exist.

- [ ] **Step 3: Update fresh schema and add the migration before Station reads**

Change the fresh `stations` schema to:

```sql
station_type TEXT NOT NULL,
website_url TEXT NOT NULL,
api_base_url TEXT NOT NULL,
endpoint_revision INTEGER NOT NULL DEFAULT 1,
api_key TEXT NOT NULL,
upstream_api_format TEXT NOT NULL DEFAULT 'auto'
```

Add `endpoint_revision INTEGER NOT NULL DEFAULT 1` to `collector_snapshots`, `collector_runs`, `station_endpoint_health`, and `station_key_health`. Call `migrate_station_endpoint_urls(&connection)` immediately after `initialize_schema` in both production initialization and `new_in_memory_for_tests`.

Backfill only relational revision columns. Do not rewrite historical `summary_json`, `normalized_json`, `raw_json_redacted`, request logs, channel-monitor runs, or change events; their legacy embedded `baseUrl` keys remain historical payload data.

Implement the fail-closed schema-state matrix from the design using `PRAGMA table_info(stations)`. Perform `ALTER TABLE ... RENAME COLUMN`, `ADD COLUMN`, backfill, validation, legacy-column removal, and derived-table revision additions inside one transaction. When both `base_url` and `api_base_url` exist, compare normalized migrated values and return an error on conflict.

- [ ] **Step 4: Update Rust models and row projections**

Use these fields consistently:

```rust
pub struct Station {
    pub id: String,
    pub name: String,
    pub station_type: String,
    pub website_url: String,
    pub api_base_url: String,
    pub endpoint_revision: i64,
    // existing non-endpoint fields remain unchanged
}

pub struct KeyPoolItem {
    // existing identity fields
    pub station_api_base_url: String,
    pub station_endpoint_revision: i64,
    // existing Key fields
}
```

Add `endpoint_revision` to `CollectorSnapshot` and `CollectorRun`, add `COLLECTOR_RUN_SUPERSEDED`, and change `StationLoginTestInput.base_url` to `website_url`. Update every constructor and field read in the files listed for this task. Password login, Web authorization, cookies, and management calls use `website_url`; OpenAI model/probe/routing calls and `station_api_base_url` projections use `api_base_url`. For a one-server fixture, set both roles to the fixture origin and make the API value end in `/v1`; use `endpoint_revision: 1`. Do not add legacy alias fields.

- [ ] **Step 5: Update create, update, list, due-collector, Key Pool, and route SQL**

Replace Station selects with explicit `website_url, api_base_url, endpoint_revision`. Populate route candidates and Key Pool items only from `api_base_url`; never select the website into an upstream field. Remove positional select gaps for `upstream_api_base_path` and adjust row indexes together with each query.

Before INSERT/UPDATE, call:

```rust
let endpoints = normalize_station_endpoints(&input.website_url, &input.api_base_url)?;
```

Store both normalized values in the same SQL statement.

- [ ] **Step 6: Run database and model tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml station_endpoint_migration -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml station_create_round_trips_both_urls -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml collector_run_serializes_camel_case -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: focused tests PASS and `cargo check` reports no legacy Rust field references.

- [ ] **Step 7: Commit the schema and contracts**

```powershell
git add -- src-tauri/src/models/stations.rs src-tauri/src/models/station_keys.rs src-tauri/src/models/collector.rs src-tauri/src/models/collector_runs.rs src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs src-tauri/src/services/channel_monitors/mod.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/collectors/sub2api.rs src-tauri/src/services/collectors/apply.rs src-tauri/src/services/collectors/adapters/openai_compatible.rs src-tauri/src/services/collectors/adapters/newapi/auth.rs src-tauri/src/services/collectors/adapters/newapi/client.rs src-tauri/src/services/collectors/adapters/newapi/mod.rs src-tauri/src/services/collectors/adapters/sub2api.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/router.rs src-tauri/src/services/proxy/routing_snapshot.rs src-tauri/src/services/remote_keys.rs
git commit -m "feat: persist station website and API URLs"
```

## Task 3: Make Endpoint Updates Atomic and Safe

**Files:**
- Modify: `src-tauri/src/services/database.rs:347-390,6534-6635,8149-8355,11650-12180`
- Modify: `src-tauri/src/commands/mod.rs:82-105`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Write failing endpoint-change safeguard tests**

Add tests that seed credentials and health, then update URLs:

```rust
#[test]
fn api_origin_change_disables_station_increments_revision_and_clears_health() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "origin-change");
    seed_station_and_key_health(&database, &station.id);

    let updated = update_test_station_urls(
        &database,
        &station,
        station.website_url.clone(),
        "https://new-api.example/v1".to_string(),
        true,
    );

    assert!(!updated.enabled);
    assert_eq!(updated.endpoint_revision, station.endpoint_revision + 1);
    assert_eq!(database.get_station_endpoint_health(updated.id.clone()).unwrap().status, "unchecked");
    assert_station_key_health_cleared(&database, &updated.id);
}

#[test]
fn website_origin_change_clears_secret_login_material_but_keeps_username() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = station_with_saved_credentials(&database);
    let updated = update_test_station_urls(
        &database,
        &station,
        "https://new-console.example".to_string(),
        station.api_base_url.clone(),
        true,
    );
    let credentials = database.get_station_credentials(updated.id).unwrap();
    assert_eq!(credentials.login_username.as_deref(), Some("user@example.com"));
    assert!(!credentials.password_present);
    assert!(!credentials.cookie_present);
    assert_eq!(updated.endpoint_revision, station.endpoint_revision + 1);
}

#[test]
fn unrelated_station_edit_does_not_increment_endpoint_revision() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "rename-only");
    let updated = update_test_station_name(&database, &station, "Renamed");
    assert_eq!(updated.endpoint_revision, station.endpoint_revision);
}
```

- [ ] **Step 2: Run the safeguard tests and confirm failure**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml api_origin_change_disables -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml website_origin_change_clears -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml unrelated_station_edit_does_not_increment -- --nocapture
```

Expected: FAIL because endpoint changes have no revision or cleanup behavior.

- [ ] **Step 3: Refactor Station update into one transaction**

Compare normalized old/new URLs with `same_origin`. In one transaction:

```rust
let endpoints_changed = existing.website_url != endpoints.website_url
    || existing.api_base_url != endpoints.api_base_url;
let website_origin_changed = !same_origin(&existing.website_url, &endpoints.website_url)?;
let api_origin_changed = !same_origin(&existing.api_base_url, &endpoints.api_base_url)?;
let next_revision = existing.endpoint_revision + i64::from(endpoints_changed);
let next_enabled = input.enabled && !api_origin_changed;
```

Write the Station URLs/revision, clear `last_checked_at` and `last_pricing_fetched_at` on any endpoint change, reset collection status, and reset endpoint/Key health on API URL change. On website-origin change, delete encrypted session/token/cookie/password references and preserve only the username. Ensure transaction rollback restores both the old URLs and all dependent state.

Add the shared transaction primitive needed by later background writers:

```rust
pub(crate) fn with_station_endpoint_revision<T>(
    &self,
    station_id: &str,
    expected_revision: i64,
    operation: impl FnOnce(&Transaction<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let mut connection = self.connection()?;
    let transaction = connection.transaction()
        .map_err(|error| format!("开始站点端点事务失败: {error}"))?;
    ensure_station_endpoint_revision(&transaction, station_id, expected_revision)?;
    let output = operation(&transaction)?;
    transaction.commit()
        .map_err(|error| format!("提交站点端点事务失败: {error}"))?;
    Ok(output)
}

fn ensure_station_endpoint_revision(
    connection: &Connection,
    station_id: &str,
    expected_revision: i64,
) -> Result<(), String> {
    let current: i64 = connection.query_row(
        "SELECT endpoint_revision FROM stations WHERE id = ?1",
        params![station_id],
        |row| row.get(0),
    ).map_err(|error| format!("读取站点端点版本失败: {error}"))?;
    if current != expected_revision {
        return Err("station_endpoint_revision_changed".to_string());
    }
    Ok(())
}
```

- [ ] **Step 4: Add a command response signal for origin-change behavior**

Keep `update_station` returning `Station`. The returned `enabled = false` is the authoritative signal after API-origin change. Use categorized backend error text for invalid website and API values; do not include full credential-bearing data.

- [ ] **Step 5: Run safeguards and the existing secret tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml api_origin_change_disables -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml website_origin_change_clears -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml encrypted_station_credentials -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml station_endpoint_health_flows -- --nocapture
```

Expected: PASS; origin changes are atomic and no plaintext secrets appear.

- [ ] **Step 6: Commit endpoint update safeguards**

```powershell
git add -- src-tauri/src/services/database.rs src-tauri/src/commands/mod.rs
git commit -m "feat: guard station endpoint changes"
```

## Task 4: Route Every Upstream Consumer Through the API Namespace

**Files:**
- Modify: `src-tauri/src/services/proxy/mod.rs:45-52`
- Modify: `src-tauri/src/services/proxy/runtime.rs:885-1135,1660-1705,2580-2610`
- Modify: `src-tauri/src/services/proxy/router.rs`
- Modify: `src-tauri/src/services/endpoint_ping.rs`
- Modify: `src-tauri/src/services/channel_monitors/mod.rs:141-176,280-310,390-410`
- Modify: `src-tauri/src/services/channel_monitors/probe.rs:54-150`
- Modify: `src-tauri/src/commands/mod.rs:663-690,1731-2085`
- Modify: `src-tauri/src/services/database.rs:7670-7780,8420-8590`
- Test: `src-tauri/src/services/station_endpoints.rs`
- Test: `src-tauri/src/services/endpoint_ping.rs`
- Test: `src-tauri/src/services/channel_monitors/mod.rs`
- Test: `src-tauri/src/services/channel_monitors/probe.rs`
- Test: `src-tauri/src/services/proxy/runtime.rs`
- Test: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Write failing API-role projection and URL tests**

Add tests asserting a Station with `website_url = server A` and `api_base_url = server B/api/v3` produces:

```rust
assert_eq!(key_pool_item.station_api_base_url, api_base_url);
assert_eq!(route_candidate.upstream_base_url, api_base_url);
assert_eq!(build_api_url(&api_base_url, "/v1/responses").unwrap(), format!("{api_base_url}/responses"));
assert_eq!(build_api_url(&api_base_url, "/v1/models").unwrap(), format!("{api_base_url}/models"));
```

Add a PING fixture that fails if the website server is contacted and succeeds only on the API server.

- [ ] **Step 2: Run focused projection, proxy, and PING tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml station_api_base_url -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml builds_resources_from_complete_api_namespaces -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml ping_uses_api -- --nocapture
```

Expected: FAIL while queries and commands still consume legacy Base URL semantics.

- [ ] **Step 3: Replace the proxy URL helper with `station_endpoints::build_api_url`**

Delete `proxy::build_upstream_url` and its duplicate tests. Import `build_api_url` directly in proxy runtime, commands, endpoint PING, collector API calls, and channel monitor probes. Propagate its `Result<String, String>` as a sanitized endpoint configuration error; do not unwrap or panic.

- [ ] **Step 4: Propagate API URL and revision through candidates**

Update Key Pool, simple route, rich route, scheduler, connectivity, and channel monitor projections to select `s.api_base_url, s.endpoint_revision`. Populate `station_api_base_url`, `station_endpoint_revision`, `upstream_base_url`, and candidate revision fields consistently.

- [ ] **Step 5: Update API consumers**

- Station PING derives its target only from `api_base_url`.
- Model discovery uses `build_api_url(api_base_url, "/v1/models")`.
- Responses and Chat probes use `build_api_url`.
- Channel monitor templates continue storing local `/v1/...` paths; the builder maps them into the configured API namespace.
- Proxy forwarding and fallback use the candidate API namespace and keep logging `upstream_base_url` as the actual configured base.

- [ ] **Step 6: Run focused and proxy regression tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml builds_resources_from_complete_api_namespaces -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml station_endpoint_health_flows -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml channel_monitor -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml proxy::runtime -- --nocapture
```

Expected: PASS with no duplicated `/v1` for `/api/v3` or `/api/paas/v4` bases.

- [ ] **Step 7: Commit upstream-role wiring**

```powershell
git add -- src-tauri/src/services/proxy/mod.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/proxy/router.rs src-tauri/src/services/endpoint_ping.rs src-tauri/src/services/channel_monitors/mod.rs src-tauri/src/services/channel_monitors/probe.rs src-tauri/src/commands/mod.rs src-tauri/src/services/database.rs
git commit -m "feat: route upstream traffic through API URLs"
```

## Task 5: Split Collector and Remote-Key Endpoint Roles

**Files:**
- Modify: `src-tauri/src/services/collectors/url.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/collectors/sub2api.rs`
- Modify: `src-tauri/src/services/collectors/adapters/openai_compatible.rs`
- Modify: `src-tauri/src/services/collectors/adapters/sub2api.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/auth.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/client.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`
- Modify: `src-tauri/src/services/remote_keys.rs`
- Modify: `src-tauri/src/models/collector.rs`
- Modify: `src-tauri/src/commands/mod.rs:1028-1115`
- Test: collector adapter modules and `src-tauri/src/services/remote_keys.rs`

- [ ] **Step 1: Add a dual-origin collector fixture**

Create a test with two local servers and request ledgers:

```rust
let management = RecordingServer::new(&[
    ("POST", "/api/user/login"),
    ("GET", "/api/user/self"),
    ("GET", "/api/v1/groups/available"),
    ("GET", "/api/v1/keys"),
]);
let api = RecordingServer::new(&[
    ("GET", "/v1/models"),
    ("GET", "/v1/usage"),
]);
let station = station_with_urls(management.base_url(), format!("{}/v1", api.base_url()));
```

After collection and remote-key scan, assert all `/api/...` requests reached management and all OpenAI-compatible resources reached API.

- [ ] **Step 2: Run the dual-origin tests and confirm cross-use failure**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml dual_origin -- --nocapture
```

Expected: FAIL because `collector_base_urls` still derives both roles from one field.

- [ ] **Step 3: Replace derivation with explicit endpoint arguments**

Use `build_management_url(&station.website_url, path)` for password login, identity, balance/group/rate management APIs, status APIs, and remote key CRUD. Use `build_api_url(&station.api_base_url, path)` for OpenAI `/models`, upstream `/usage`, and any probe carrying an API key.

Rename generic parameters where their role is fixed:

```rust
fn request_password_login(
    website_url: &str,
    login_username: &str,
    login_password: &str,
) -> Result<NewApiLogin, String>
```

Change `StationLoginTestInput` and its frontend contract to `websiteUrl`; return `missing_website_url` for empty input.

- [ ] **Step 4: Remove `CollectorBaseUrls`**

After `rg "collector_base_urls|CollectorBaseUrls|collectors::url" src-tauri/src` returns only `collectors/url.rs` and its module declaration, delete `src-tauri/src/services/collectors/url.rs` and remove `pub mod url` from `collectors/mod.rs`. All URL joins use `build_management_url` or `build_api_url` from `station_endpoints`.

- [ ] **Step 5: Run collector and remote-key regressions**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml dual_origin -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml collectors::adapters::newapi -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml collectors::adapters::sub2api -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml remote_keys -- --nocapture
```

Expected: PASS and management/API request ledgers remain disjoint.

- [ ] **Step 6: Commit collector-role wiring**

```powershell
git add -- src-tauri/src/services/collectors/url.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/collectors/sub2api.rs src-tauri/src/services/collectors/adapters/openai_compatible.rs src-tauri/src/services/collectors/adapters/sub2api.rs src-tauri/src/services/collectors/adapters/newapi/auth.rs src-tauri/src/services/collectors/adapters/newapi/client.rs src-tauri/src/services/collectors/adapters/newapi/mod.rs src-tauri/src/services/remote_keys.rs src-tauri/src/models/collector.rs src-tauri/src/commands/mod.rs
git commit -m "feat: separate management and collector API traffic"
```

## Task 6: Harden Authorization for Dual Origins and Endpoint Revisions

**Files:**
- Modify: `src-tauri/src/services/capture/session.rs`
- Modify: `src-tauri/src/services/capture/web_authorization.rs`
- Modify: `src-tauri/src/commands/mod.rs:1137-1395`
- Test: `src-tauri/src/services/capture/session.rs`
- Test: `src-tauri/src/services/capture/web_authorization.rs`
- Test: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Add failing URL-boundary and stale-session tests**

```rust
#[test]
fn capture_accepts_configured_origins_and_rejects_lookalikes() {
    assert!(capture_request_belongs_to_station(
        "https://console.example",
        "https://api.example/v1",
        "https://console.example/api/user/self",
    ));
    assert!(capture_request_belongs_to_station(
        "https://console.example",
        "https://api.example/v1",
        "https://api.example/v1/models",
    ));
    assert!(!capture_request_belongs_to_station(
        "https://console.example",
        "https://api.example/v1",
        "https://console.example.evil.test/api/user/self",
    ));
}

#[test]
fn capture_session_retains_its_start_revision() {
    let store = CaptureSessionStore::default();
    store.start("station-1".into(), "capture-station-1".into(), 4).unwrap();
    assert_eq!(store.endpoint_revision("station-1").unwrap(), Some(4));
}
```

- [ ] **Step 2: Run focused capture tests and confirm failure**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml capture_accepts_configured_origins -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml capture_session_retains -- --nocapture
```

Expected: FAIL because capture matching takes one Base URL and sessions do not store revision.

- [ ] **Step 3: Store revision internally in capture-session state**

Change `CaptureSessionStore::start` to accept `endpoint_revision: i64`; store it in `CaptureSession` and add an internal `endpoint_revision(station_id)` accessor for finish/record paths. Do not add it to `CaptureSessionStatus` or expose it to frontend code.

- [ ] **Step 4: Use website navigation and structured dual-origin matching**

- Open authorization WebViews at `station.website_url`.
- Read cookies for the parsed website origin.
- Verify NewAPI sessions against management URLs built from `website_url`.
- Accept captured requests belonging to either configured base using `url_belongs_to_base`.
- Remove the raw string-prefix matcher.

- [ ] **Step 5: Gate credential persistence on revision**

Add `persist_station_session_if_revision(input, expected_revision, data_key)` in `database.rs` and implement it with `with_station_endpoint_revision` from Task 3. Call it before password/session status updates, return a sanitized `endpoint_revision_changed` result, and persist nothing when stale. Add a command-level test that starts capture at revision `4`, updates the Station to revision `5`, calls finish, and verifies no credential row was created. Endpoint edits clear/close the active window in the UI; the revision check remains authoritative.

- [ ] **Step 6: Run capture and authorization regressions**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml capture -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml web_authorization -- --nocapture
node --test scripts/manual-authorization-capability.test.mjs
node --test scripts/automatic-web-authorization-completion.test.mjs
```

Expected: PASS; existing automatic authorization behavior remains, with user work in `web_authorization.rs` preserved.

- [ ] **Step 7: Commit authorization hardening**

```powershell
git add -- src-tauri/src/services/capture/session.rs src-tauri/src/services/capture/web_authorization.rs src-tauri/src/commands/mod.rs
git commit -m "feat: bind authorization to station endpoints"
```

## Task 7: Reject Stale Background Writes and Credential Redirects

**Files:**
- Modify: `src-tauri/src/models/collector_runs.rs`
- Modify: `src-tauri/src/services/database.rs:1218-1238,1580-1593,8149-8355,11705-12680`
- Modify: `src-tauri/src/services/collectors/apply.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/channel_monitors/mod.rs`
- Modify: `src-tauri/src/services/channel_monitors/probe.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/outbound.rs`
- Modify: `src-tauri/src/services/collectors/sub2api.rs`
- Modify: `src-tauri/src/services/collectors/adapters/openai_compatible.rs`
- Modify: `src-tauri/src/services/collectors/adapters/sub2api.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/auth.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/client.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi/mod.rs`
- Test: `src-tauri/src/services/database.rs`
- Test: `src-tauri/src/services/collectors/apply.rs`
- Test: `src-tauri/src/services/collectors/mod.rs`
- Test: `src-tauri/src/services/channel_monitors/mod.rs`
- Test: `src-tauri/src/services/proxy/runtime.rs`
- Test: `src-tauri/src/services/outbound.rs`

- [ ] **Step 1: Write failing stale-result tests**

```rust
#[test]
fn collector_output_from_old_revision_finishes_superseded_without_applying_facts() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station = test_station(&database, "stale-collector");
    let output = successful_balance_output(&station.id, 99.0);
    bump_station_endpoint_revision(&database, &station.id);

    let applied = apply_adapter_output(&database, &station.id, station.endpoint_revision, None, output)
        .expect("record superseded output");

    assert_eq!(applied.run.status, COLLECTOR_RUN_SUPERSEDED);
    assert!(database.list_current_station_balance_snapshots().unwrap().is_empty());
}

#[test]
fn old_proxy_feedback_does_not_restore_cleared_key_health() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let candidate = candidate_at_revision(&database, 1);
    bump_station_endpoint_revision(&database, &candidate.station_id);
    record_candidate_failure(&database, &candidate, "old failure").unwrap();
    assert_station_key_health_cleared(&database, &candidate.station_id);
}
```

- [ ] **Step 2: Write a failing redirect-safety test**

Use a source server that redirects to a second server and records headers. Send a request with `Authorization: Bearer canary-secret` through the credential-safe agent. Assert the second server receives no request and no canary appears in the returned error.

- [ ] **Step 3: Run focused tests and confirm failure**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml superseded -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml old_proxy_feedback -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml credential_redirect -- --nocapture
```

Expected: FAIL because writes and redirects are not revision/origin guarded.

- [ ] **Step 4: Extend the shared revision transaction to every current-state writer**

Use `with_station_endpoint_revision` from Task 3 for current collector facts/status, Station/Key health, authorization persistence, and proxy feedback. The held database mutex prevents an endpoint update from interleaving between the revision check and commit. Refactor collector snapshot/run/fact writers into `_in_connection` helpers so `apply_adapter_output` applies the snapshot, normalized facts, Station freshness, and final run status through one revision transaction without recursively locking `AppDatabase`. Store revision on snapshots/runs/health. Health reads join on matching Station revision.

- [ ] **Step 5: Mark stale collector runs as superseded**

Pass the captured revision into `apply_adapter_output`. Insert the historical snapshot with that revision, skip `apply_collector_facts`, finish the run with `COLLECTOR_RUN_SUPERSEDED`, and do not emit failure/recovery change events or update Station freshness.

- [ ] **Step 6: Add a no-redirect credential agent**

In `outbound.rs`:

```rust
pub fn credential_agent_builder_for_proxy(
    proxy: &ProxyConfig,
) -> Result<ureq::AgentBuilder, String> {
    Ok(agent_builder_for_proxy(proxy)?.redirects(0))
}
```

Use it for requests carrying Station passwords, cookies, access tokens, API Keys, or Station Keys. Treat 3xx as a sanitized endpoint error or relay it without following; never manually copy a credential header to a different origin.

- [ ] **Step 7: Run stale-write, redirect, collector, monitor, and proxy tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml superseded -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml credential_redirect -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml collector_run -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml channel_monitor -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml proxy::runtime -- --nocapture
```

Expected: PASS; old work remains historical and cannot repopulate current state.

- [ ] **Step 8: Commit concurrency and redirect safety**

```powershell
git add -- src-tauri/src/models/collector_runs.rs src-tauri/src/services/database.rs src-tauri/src/services/collectors/apply.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/collectors/sub2api.rs src-tauri/src/services/collectors/adapters/openai_compatible.rs src-tauri/src/services/collectors/adapters/sub2api.rs src-tauri/src/services/collectors/adapters/newapi/auth.rs src-tauri/src/services/collectors/adapters/newapi/client.rs src-tauri/src/services/collectors/adapters/newapi/mod.rs src-tauri/src/services/channel_monitors/mod.rs src-tauri/src/services/channel_monitors/probe.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/outbound.rs
git commit -m "feat: reject stale station endpoint work"
```

## Task 8: Update Frontend Contracts, Memory Fallback, and Presets

**Files:**
- Create: `scripts/station-endpoint-contract.test.mjs`
- Modify: `src/lib/types/stations.ts`
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/lib/types/collector.ts`
- Modify: `src/lib/api/stations.ts`
- Modify: `src/lib/api/stationKeys.ts`
- Modify: `src/lib/api/collector.ts`
- Modify: `src/lib/mock/stations.ts`
- Modify: `src/features/stations/providerPresets.ts`
- Modify: `scripts/add-provider-preset-types.test.mjs`
- Modify: `src/features/stations/AddProviderPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/stations/components/StationDetailContent.tsx`
- Modify: `src/features/stations/components/StationDetailPanel.tsx`
- Modify: `src/features/key-pool/AddKeyPage.tsx`
- Modify: `src/features/key-pool/EditKeyPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Modify: `src/features/pricing/PricingPage.tsx`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/lib/projections/runtimeSnapshot.ts`
- Modify: `scripts/group-category-contract.test.mjs`
- Modify: `scripts/pricing-facts-projection.test.mjs`
- Modify: `scripts/pricing-group-comparison-view-model.test.mjs`
- Modify: `scripts/pricing-station-browser-link.test.mjs`
- Modify: `scripts/runtime-snapshot-boundary.test.mjs`
- Modify: `scripts/runtime-snapshot-projection.test.mjs`
- Modify: `scripts/station-assets-current-projections.test.mjs`
- Modify: `scripts/station-current-balance-projection.test.mjs`
- Modify: `scripts/station-list-risk-tags.test.mjs`
- Modify: `scripts/station-url-browser-link.test.mjs`

- [ ] **Step 1: Write the failing frontend endpoint contract test**

Create `scripts/station-endpoint-contract.test.mjs`:

```javascript
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const stationTypes = await readFile("src/lib/types/stations.ts", "utf8");
const stationApi = await readFile("src/lib/api/stations.ts", "utf8");
const keyTypes = await readFile("src/lib/types/stationKeys.ts", "utf8");
const presets = await readFile("src/features/stations/providerPresets.ts", "utf8");
const pricing = await readFile("src/features/pricing/PricingPage.tsx", "utf8");
const collectors = await readFile("src/features/collectors/CollectorsPage.tsx", "utf8");
const dashboard = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const keyPool = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");
const runtimeSnapshot = await readFile("src/lib/projections/runtimeSnapshot.ts", "utf8");

test("station contracts expose explicit website and API fields", () => {
  assert.match(stationTypes, /websiteUrl: string/);
  assert.match(stationTypes, /apiBaseUrl: string/);
  assert.match(stationTypes, /endpointRevision: number/);
  assert.doesNotMatch(stationTypes, /^\s*baseUrl: string/m);
  assert.match(keyTypes, /stationApiBaseUrl: string/);
  assert.doesNotMatch(keyTypes, /stationBaseUrl: string/);
});

test("memory fallback and presets carry both endpoint roles", () => {
  assert.match(stationApi, /websiteUrl: input\.websiteUrl/);
  assert.match(stationApi, /apiBaseUrl: input\.apiBaseUrl/);
  assert.match(presets, /websiteUrl: string/);
  assert.match(presets, /apiBaseUrl: string/);
});

test("views and runtime projections consume the correct endpoint role", () => {
  assert.match(pricing, /openStationWebsite/);
  assert.match(pricing, /websiteUrl/);
  assert.match(collectors, /websiteUrl/);
  assert.match(dashboard, /stationApiBaseUrl/);
  assert.match(keyPool, /stationApiBaseUrl/);
  assert.match(runtimeSnapshot, /upstreamBaseUrl: station\.apiBaseUrl/);
  assert.match(runtimeSnapshot, /endpointRevision: station\.endpointRevision/);
});
```

- [ ] **Step 2: Run the contract test and confirm failure**

Run:

```powershell
node --test scripts/station-endpoint-contract.test.mjs
```

Expected: FAIL on missing explicit fields and remaining ambiguous types.

- [ ] **Step 3: Replace TypeScript Station contracts**

Use:

```typescript
export type Station = {
  id: string;
  name: string;
  stationType: StationType;
  websiteUrl: string;
  apiBaseUrl: string;
  endpointRevision: number;
  // existing non-endpoint fields
};

export type StationInput = {
  name: string;
  stationType: StationType;
  websiteUrl: string;
  apiBaseUrl: string;
  apiKey: string;
  // existing non-endpoint fields
};
```

Rename `stationBaseUrl` to `stationApiBaseUrl` and add `stationEndpointRevision` to Station-derived Key targets. Rename login-test input `baseUrl` to `websiteUrl` and add revision fields to collector data types where serialized by Rust.

- [ ] **Step 4: Update memory fallback and API wrappers**

Create/update memory Stations with both URLs and increment `endpointRevision` only when either normalized string changes. Rename `openStationBaseUrl` to `openStationWebsite`. Populate `stationApiBaseUrl` from `station.apiBaseUrl` in the Station Key fallback and add endpoint revisions to fallback snapshots/health.

- [ ] **Step 5: Give every preset explicit values**

Change `ProviderPreset` to:

```typescript
export type ProviderPreset = {
  id: ProviderPresetId;
  name: string;
  description: string;
  stationType: StationType;
  websiteUrl: string;
  apiBaseUrl: string;
};
```

Use these explicit pairs and retain the existing API namespace values:

```typescript
const presetEndpoints = {
  custom: ["", ""],
  kamiapi: ["https://www.kamiapi.top", "https://www.kamiapi.top/v1"],
  deepseek: ["https://platform.deepseek.com", "https://api.deepseek.com/v1"],
  qwen: ["https://bailian.console.aliyun.com", "https://dashscope.aliyuncs.com/compatible-mode/v1"],
  zhipu: ["https://open.bigmodel.cn", "https://open.bigmodel.cn/api/paas/v4"],
  kimi: ["https://platform.moonshot.cn", "https://api.moonshot.ai/v1"],
  doubao: ["https://console.volcengine.com/ark", "https://ark.cn-beijing.volces.com/api/v3"],
  hunyuan: ["https://cloud.tencent.com/product/hunyuan", "https://api.hunyuan.cloud.tencent.com/v1"],
  qianfan: ["https://console.bce.baidu.com/qianfan/ais/console", "https://qianfan.baidubce.com/v2"],
  siliconflow: ["https://cloud.siliconflow.cn", "https://api.siliconflow.cn/v1"],
  minimax: ["https://platform.minimaxi.com", "https://api.minimax.io/v1"],
  stepfun: ["https://platform.stepfun.com", "https://api.stepfun.com/v1"],
  mimo: ["https://platform.xiaomimimo.com", "https://api.xiaomimimo.com/v1"],
  lingyiwanwu: ["https://platform.lingyiwanwu.com", "https://api.lingyiwanwu.com/v1"],
  baichuan: ["https://platform.baichuan-ai.com", "https://api.baichuan-ai.com/v1"],
} as const;
```

Copy the literal values into `providerPresets`; do not derive one field from the other at runtime.

- [ ] **Step 6: Update every frontend consumer and fixture to the new property names**

Mechanically replace Station-domain reads according to role:

- Add Provider and Stations forms submit both fields.
- Station details and browser links read `websiteUrl`.
- Key Pool, Dashboard, Add/Edit Key, and runtime snapshot read `apiBaseUrl` or `stationApiBaseUrl`.
- Pricing browser links and Collector descriptions read `websiteUrl`.
- The nine listed script fixtures provide `websiteUrl`, `apiBaseUrl`, and `endpointRevision: 1`.

Do not add a deprecated `baseUrl` alias. This step is complete only when `pnpm build` passes; Task 9 adds the final labels, warnings, copy action, and read-only presentation.

- [ ] **Step 7: Run frontend contract and preset tests**

Run:

```powershell
node --test scripts/station-endpoint-contract.test.mjs
node --test scripts/add-provider-preset-types.test.mjs
node --test scripts/runtime-snapshot-boundary.test.mjs
node --test scripts/runtime-snapshot-projection.test.mjs
pnpm build
```

Expected: endpoint contract, preset, and runtime projection tests PASS; `pnpm build` exits 0.

- [ ] **Step 8: Commit frontend contracts**

```powershell
git add -- scripts/station-endpoint-contract.test.mjs scripts/add-provider-preset-types.test.mjs scripts/group-category-contract.test.mjs scripts/pricing-facts-projection.test.mjs scripts/pricing-group-comparison-view-model.test.mjs scripts/pricing-station-browser-link.test.mjs scripts/runtime-snapshot-boundary.test.mjs scripts/runtime-snapshot-projection.test.mjs scripts/station-assets-current-projections.test.mjs scripts/station-current-balance-projection.test.mjs scripts/station-list-risk-tags.test.mjs scripts/station-url-browser-link.test.mjs src/lib/types/stations.ts src/lib/types/stationKeys.ts src/lib/types/collector.ts src/lib/api/stations.ts src/lib/api/stationKeys.ts src/lib/api/collector.ts src/lib/mock/stations.ts src/features/stations/providerPresets.ts src/features/stations/AddProviderPage.tsx src/features/stations/StationsPage.tsx src/features/stations/components/StationDetailContent.tsx src/features/stations/components/StationDetailPanel.tsx src/features/key-pool/AddKeyPage.tsx src/features/key-pool/EditKeyPage.tsx src/features/key-pool/KeyPoolPage.tsx src/features/dashboard/DashboardPage.tsx src/features/pricing/PricingPage.tsx src/features/collectors/CollectorsPage.tsx src/lib/projections/runtimeSnapshot.ts
git commit -m "feat: expose station endpoint contracts in frontend"
```

## Task 9: Update Station and Key Editing Experiences

**Files:**
- Create: `scripts/station-endpoint-ui.test.mjs`
- Modify: `src/features/stations/AddProviderPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/stations/components/StationDetailContent.tsx`
- Modify: `src/features/stations/components/StationDetailPanel.tsx`
- Modify: `src/features/key-pool/AddKeyPage.tsx`
- Modify: `src/features/key-pool/EditKeyPage.tsx`
- Test: existing Station/Key source-contract scripts

- [ ] **Step 1: Write the failing UI source contract**

Create a Node test asserting both forms contain `websiteUrl` and `apiBaseUrl`, detail components contain `前端网址` and `API Base URL`, the Station browser action receives `websiteUrl`, and Add Key contains `stationApiBaseUrl` without an editable Station URL input.

```javascript
test("station forms and details keep endpoint roles distinct", () => {
  for (const source of [addProvider, stationsPage]) {
    assert.match(source, /websiteUrl/);
    assert.match(source, /apiBaseUrl/);
  }
  assert.match(stationDetails, /前端网址/);
  assert.match(stationDetails, /API Base URL/);
  assert.match(stationsPage, /openStationWebsite\(station\.websiteUrl\)/);
  assert.doesNotMatch(addKeyPage, /onChange=.*baseUrl/);
});
```

- [ ] **Step 2: Run the UI test and confirm failure**

Run:

```powershell
node --test scripts/station-endpoint-ui.test.mjs
```

Expected: FAIL on single-field forms and the editable Add Key Base URL.

- [ ] **Step 3: Update Add Provider and Station dialog form state**

Use explicit fields:

```typescript
type StationEndpointForm = {
  websiteUrl: string;
  apiBaseUrl: string;
};
```

Render adjacent required fields labeled `前端网址` and `API Base URL`. Add a small icon copy action with tooltip `复制前端网址` that performs one assignment:

```typescript
setForm((current) => ({ ...current, apiBaseUrl: current.websiteUrl }));
```

It must not create a synchronized toggle. Login connection tests submit `websiteUrl`; save submits both values.

- [ ] **Step 4: Add endpoint-origin change warnings**

Before updating an existing Station, parse old/new origins in a frontend helper used only for warning copy. Website-origin warning says saved login state will be cleared. API-origin warning says the Station will be disabled and existing Keys will not route until the user validates and re-enables it. Backend behavior remains authoritative.

On successful endpoint update, cancel/close the active authorization UI and invalidate Station, Key Pool, routing workspace, collector, endpoint health, and channel monitor query families.

- [ ] **Step 5: Update list and detail presentation**

Use `websiteUrl` as the clickable row link. Show a compact secondary `API` line containing `apiBaseUrl`. Detail components show full values in separate property rows. PING help/title copy identifies the API endpoint.

- [ ] **Step 6: Make Key endpoint context read-only**

In Add Key, replace the editable ignored `baseUrl` field with `stationApiBaseUrl` derived from the selected Station and rendered `readOnly` or disabled. Edit Key and Key detail context use the renamed property. Station Key save inputs must not include either Station URL.

- [ ] **Step 7: Run UI, Station, Key, and build checks**

Run:

```powershell
node --test scripts/station-endpoint-ui.test.mjs
node --test scripts/station-url-browser-link.test.mjs
node --test scripts/edit-key-group-binding-display.test.mjs
node --test scripts/station-list-risk-tags.test.mjs
pnpm build
```

Expected: PASS; no displayed or editable field swaps website/API roles.

- [ ] **Step 8: Commit Station and Key UI changes**

```powershell
git add -- scripts/station-endpoint-ui.test.mjs src/features/stations/AddProviderPage.tsx src/features/stations/StationsPage.tsx src/features/stations/components/StationDetailContent.tsx src/features/stations/components/StationDetailPanel.tsx src/features/key-pool/AddKeyPage.tsx src/features/key-pool/EditKeyPage.tsx
git commit -m "feat: edit station website and API URLs separately"
```

## Task 10: Update Documentation and Run End-to-End Verification

**Files:**
- Modify: `docs/PROJECT_PLAN.md`
- Modify: `README.md`
- Test: full frontend and Rust suites

- [ ] **Step 1: Update product terminology**

Change the Station definition to include:

```text
- website_url: browser login and management endpoint origin
- api_base_url: complete OpenAI-compatible API namespace used by Station Keys
```

Replace information-architecture references to a single Base URL. Document that management endpoint selection follows endpoint role, not the collector task name.

- [ ] **Step 2: Run literal ambiguity and secret-safety scans**

Run:

```powershell
rg -n "station\.base_url|station\.baseUrl|station_base_url|stationBaseUrl|CollectorBaseUrls|collector_base_urls" src src-tauri/src scripts
```

Expected: no Station-domain matches. Generic local test-server variables and the precise runtime/log term `upstream_base_url` may remain.

Run:

```powershell
rg -n "api_key|cookie|password|Authorization" docs/superpowers/specs/2026-07-13-station-website-api-url-separation-design.md docs/PROJECT_PLAN.md README.md
```

Expected: documentation mentions only field/category names and safety rules; no real secret values.

- [ ] **Step 3: Run every Node source-contract test**

Run:

```powershell
Get-ChildItem scripts -Filter *.test.mjs | ForEach-Object { node --test $_.FullName }
```

Expected: all scripts PASS.

- [ ] **Step 4: Run frontend production verification**

Run:

```powershell
pnpm build
```

Expected: `tsc --noEmit` and Vite production build PASS.

- [ ] **Step 5: Run Rust formatting, tests, and check**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all commands exit 0.

- [ ] **Step 6: Run dual-origin manual QA in Tauri**

Start:

```powershell
pnpm tauri:dev
```

Verify one Station with different website/API origins:

1. Station list opens the website origin.
2. Login/authorization uses the website origin.
3. Management collection and remote Key operations hit the website origin.
4. PING, Key test, channel monitor, and proxy requests hit the API origin.
5. Changing the website origin clears saved authorization and closes an active authorization window.
6. Changing the API origin disables the Station and clears current health.
7. A request started before an endpoint edit remains historical and does not restore current health afterward.

Expected: every behavior matches the design and no UI text overlaps at the standard desktop window size.

- [ ] **Step 7: Commit documentation and final verification fixes**

```powershell
git add -- docs/PROJECT_PLAN.md README.md
git commit -m "docs: document station endpoint roles"
```

Task 10 starts only after Tasks 1-9 are green. A failure in this final verification returns execution to the owning task and its explicit commit scope before the documentation commit is created.

- [ ] **Step 8: Record final evidence**

Capture the successful command names and test counts in the implementation handoff. Report any environment-only test that could not run with its exact error; do not claim it passed.
