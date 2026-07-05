# Remote Station Key Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add remote key discovery, matching, group/rate sync, remote key creation, and compact supplier-form Station Key editing for Sub2API/NewAPI stations.

**Architecture:** Keep `Station` as the site account and `Station Key` as the local routing object. Add a remote-key discovery fact layer plus capability-based adapter functions, then let the UI show remote facts separately from editable local keys. Sub2API gets the first concrete adapter path; NewAPI reports partial support unless a compatible deployment exposes key endpoints.

**Tech Stack:** Tauri 2, Rust, rusqlite, serde, ureq, React 18, TypeScript, Vite, Tailwind CSS, lucide-react.

---

## File Structure

- Create `src-tauri/src/models/remote_keys.rs`: serializable remote-key capability, scan, match, and create DTOs shared by commands and adapters.
- Modify `src-tauri/src/models/mod.rs`: export `remote_keys`.
- Create `src-tauri/src/services/remote_keys.rs`: pure matching, fingerprinting, masked-key helpers, capability dispatch, and high-level service functions.
- Modify `src-tauri/src/services/mod.rs`: export `remote_keys`.
- Modify `src-tauri/src/services/database.rs`: add remote-key discovery table, CRUD helpers, and match-confirm/sync helpers.
- Modify `src-tauri/src/services/collectors/adapters/mod.rs`: define remote-key adapter capability trait-style functions and shared endpoint result types.
- Modify `src-tauri/src/services/collectors/adapters/sub2api.rs`: implement Sub2API capability, remote key listing, remote key creation, and group/rate join.
- Modify `src-tauri/src/services/collectors/adapters/newapi.rs`: implement capability fallback and optional compatible endpoint parsing.
- Modify `src-tauri/src/commands/mod.rs`: add Tauri commands for capabilities, scan, create, bind, and list discovered remote keys.
- Modify `src-tauri/src/lib.rs`: register new commands.
- Modify `src/lib/types/stationKeys.ts`: add remote-key capability, discovery, create, and bind types.
- Modify `src/lib/api/stationKeys.ts`: add frontend wrappers and browser-preview memory fallback for remote-key actions.
- Modify `src/features/stations/AddProviderPage.tsx`: add compact multi-row local Station Key editor, remote discovery list, scan action, create remote key action, and save orchestration.
- Modify `src/features/stations/StationsPage.tsx`: add compact supplier row create-key action when this page still renders row actions in the active branch.
- Create `src/features/stations/components/StationKeyRowsEditor.tsx`: focused local key row editor for the supplier form.
- Create `src/features/stations/components/RemoteKeyDiscoveryList.tsx`: focused remote discovery display and bind/import actions.
- Create `src/features/stations/components/CreateRemoteKeyDialog.tsx`: group-select create dialog.
- Test with Rust unit tests in `remote_keys.rs`, database tests in `database.rs`, and TypeScript/Vite build checks.

---

### Task 1: Remote Key Types and Matching Primitives

**Files:**
- Create: `src-tauri/src/models/remote_keys.rs`
- Create: `src-tauri/src/services/remote_keys.rs`
- Modify: `src-tauri/src/models/mod.rs`
- Modify: `src-tauri/src/services/mod.rs`

- [ ] **Step 1: Add model module exports**

Modify `src-tauri/src/models/mod.rs`:

```rust
pub mod remote_keys;
```

Modify `src-tauri/src/services/mod.rs`:

```rust
pub mod remote_keys;
```

- [ ] **Step 2: Create remote key DTOs**

Create `src-tauri/src/models/remote_keys.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteKeyCapability {
    pub station_id: String,
    pub station_type: String,
    pub can_list_remote_keys: bool,
    pub can_create_remote_key: bool,
    pub can_read_groups: bool,
    pub requires_manual_session: bool,
    pub unsupported_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteStationKey {
    pub id: String,
    pub station_id: String,
    pub remote_key_id_hash: Option<String>,
    pub remote_key_name: Option<String>,
    pub api_key_masked: Option<String>,
    pub api_key_fingerprint: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub rate_multiplier: Option<f64>,
    pub rate_source: Option<String>,
    pub created_at: Option<String>,
    pub last_used_at: Option<String>,
    pub raw_source: String,
    pub match_status: RemoteKeyMatchStatus,
    pub matched_station_key_id: Option<String>,
    pub match_confidence: f64,
    pub collected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RemoteKeyMatchStatus {
    Matched,
    Possible,
    Unbound,
}

impl RemoteKeyMatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RemoteKeyMatchStatus::Matched => "matched",
            RemoteKeyMatchStatus::Possible => "possible",
            RemoteKeyMatchStatus::Unbound => "unbound",
        }
    }

    pub fn from_str(value: &str) -> Self {
        match value {
            "matched" => RemoteKeyMatchStatus::Matched,
            "possible" => RemoteKeyMatchStatus::Possible,
            _ => RemoteKeyMatchStatus::Unbound,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteKeyScanResult {
    pub station_id: String,
    pub capability: RemoteKeyCapability,
    pub keys: Vec<RemoteStationKey>,
    pub synced_station_key_ids: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRemoteStationKeyInput {
    pub station_id: String,
    pub name: String,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRemoteStationKeyResult {
    pub remote_key: RemoteStationKey,
    pub station_key: crate::models::station_keys::StationKey,
    pub full_key_once: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BindRemoteStationKeyInput {
    pub remote_key_id: String,
    pub station_key_id: String,
}
```

- [ ] **Step 3: Write matching primitive tests**

Create `src-tauri/src/services/remote_keys.rs` with tests first:

```rust
use sha2::{Digest, Sha256};

pub fn api_key_fingerprint(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    Some(format!("{:x}", hasher.finalize()))
}

pub fn visible_mask_parts(masked: &str) -> Option<(String, String)> {
    let trimmed = masked.trim();
    let Some((prefix, suffix)) = trimmed.split_once("****") else {
        return None;
    };
    let prefix = prefix.trim().to_string();
    let suffix = suffix.trim().to_string();
    if prefix.len() < 3 || suffix.len() < 3 {
        return None;
    }
    Some((prefix, suffix))
}

pub fn masked_key_matches_full(masked: &str, full_key: &str) -> bool {
    visible_mask_parts(masked)
        .map(|(prefix, suffix)| full_key.starts_with(&prefix) && full_key.ends_with(&suffix))
        .unwrap_or(false)
}

pub fn remote_key_confidence(
    remote_fingerprint: Option<&str>,
    local_fingerprint: Option<&str>,
    remote_masked: Option<&str>,
    local_full_key: Option<&str>,
    same_group: bool,
    same_name: bool,
) -> f64 {
    if remote_fingerprint.is_some() && remote_fingerprint == local_fingerprint {
        return 1.0;
    }
    if let (Some(masked), Some(full_key)) = (remote_masked, local_full_key) {
        if masked_key_matches_full(masked, full_key) {
            return if same_group || same_name { 0.92 } else { 0.82 };
        }
    }
    match (same_group, same_name) {
        (true, true) => 0.72,
        (true, false) | (false, true) => 0.55,
        (false, false) => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_identical_keys_consistently() {
        assert_eq!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-a"));
        assert_ne!(api_key_fingerprint("sk-a"), api_key_fingerprint("sk-b"));
        assert_eq!(api_key_fingerprint("   "), None);
    }

    #[test]
    fn masked_key_match_requires_visible_prefix_and_suffix() {
        assert!(masked_key_matches_full("sk-live****cdef", "sk-live-123-cdef"));
        assert!(!masked_key_matches_full("sk-live****zzzz", "sk-live-123-cdef"));
        assert!(!masked_key_matches_full("sk****ef", "sk-live-123-cdef"));
    }

    #[test]
    fn confidence_separates_high_and_possible_matches() {
        let fp = api_key_fingerprint("sk-live-123-cdef");
        assert_eq!(
            remote_key_confidence(fp.as_deref(), fp.as_deref(), None, None, false, false),
            1.0
        );
        assert!(
            remote_key_confidence(None, None, Some("sk-live****cdef"), Some("sk-live-123-cdef"), true, false)
                >= 0.9
        );
        assert!(
            remote_key_confidence(None, None, None, None, true, true) < 0.8
        );
    }
}
```

- [ ] **Step 4: Run primitive tests**

Run:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml remote_keys
```

Expected: tests compile and pass. If the module export was missed, Rust fails with an unresolved module error; add the export and rerun.

- [ ] **Step 5: Commit Task 1**

```powershell
git add -- src-tauri/src/models/mod.rs src-tauri/src/models/remote_keys.rs src-tauri/src/services/mod.rs src-tauri/src/services/remote_keys.rs
git commit -m "feat: add remote key matching primitives"
```

---

### Task 2: Remote Key Discovery Persistence

**Files:**
- Modify: `src-tauri/src/services/database.rs`
- Test: `src-tauri/src/services/database.rs`

- [ ] **Step 1: Add schema migration**

In `AppDatabase::initialize` and `AppDatabase::new_in_memory_for_tests`, call `migrate_remote_key_tables(&connection)?` after the existing pricing/request-log migrations:

```rust
migrate_remote_key_tables(&connection)
    .map_err(|error| format!("迁移远端 Key 表失败: {error}"))?;
```

Add the migration near other migration helpers:

```rust
fn migrate_remote_key_tables(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS remote_station_keys (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            remote_key_id_hash TEXT,
            remote_key_name TEXT,
            api_key_masked TEXT,
            api_key_fingerprint TEXT,
            group_id_hash TEXT,
            group_name TEXT,
            tier_label TEXT,
            rate_multiplier REAL,
            rate_source TEXT,
            created_at_remote TEXT,
            last_used_at TEXT,
            raw_source TEXT NOT NULL,
            match_status TEXT NOT NULL,
            matched_station_key_id TEXT,
            match_confidence REAL NOT NULL,
            collected_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_remote_station_keys_station
            ON remote_station_keys(station_id, collected_at DESC);
        CREATE INDEX IF NOT EXISTS idx_remote_station_keys_matched_key
            ON remote_station_keys(matched_station_key_id);
        "#,
    )
}
```

- [ ] **Step 2: Add database mapping helpers**

At the top of `database.rs`, import:

```rust
use crate::models::remote_keys::{RemoteKeyMatchStatus, RemoteStationKey};
```

Add methods on `impl AppDatabase`:

```rust
pub fn list_remote_station_keys(&self, station_id: String) -> Result<Vec<RemoteStationKey>, String> {
    let connection = self.connection()?;
    list_remote_station_keys_from_connection(&connection, &station_id)
}

pub fn replace_remote_station_keys(
    &self,
    station_id: String,
    keys: Vec<RemoteStationKey>,
) -> Result<Vec<RemoteStationKey>, String> {
    let connection = self.connection()?;
    let tx = connection
        .unchecked_transaction()
        .map_err(|error| format!("无法开始远端 Key 写入事务: {error}"))?;
    tx.execute(
        "DELETE FROM remote_station_keys WHERE station_id = ?1",
        rusqlite::params![station_id],
    )
    .map_err(|error| format!("无法清理旧远端 Key: {error}"))?;
    for key in keys {
        insert_remote_station_key(&tx, &key)?;
    }
    tx.commit()
        .map_err(|error| format!("无法提交远端 Key 写入事务: {error}"))?;
    list_remote_station_keys_from_connection(&connection, &station_id)
}
```

Add free helpers:

```rust
fn list_remote_station_keys_from_connection(
    connection: &Connection,
    station_id: &str,
) -> Result<Vec<RemoteStationKey>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, station_id, remote_key_id_hash, remote_key_name, api_key_masked,
                    api_key_fingerprint, group_id_hash, group_name, tier_label,
                    rate_multiplier, rate_source, created_at_remote, last_used_at,
                    raw_source, match_status, matched_station_key_id, match_confidence,
                    collected_at
             FROM remote_station_keys
             WHERE station_id = ?1
             ORDER BY collected_at DESC, remote_key_name COLLATE NOCASE",
        )
        .map_err(|error| format!("无法准备远端 Key 查询: {error}"))?;
    let rows = statement
        .query_map(rusqlite::params![station_id], remote_station_key_from_row)
        .map_err(|error| format!("无法读取远端 Key: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("无法解析远端 Key: {error}"))
}

fn insert_remote_station_key(
    connection: &Connection,
    key: &RemoteStationKey,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO remote_station_keys (
                id, station_id, remote_key_id_hash, remote_key_name, api_key_masked,
                api_key_fingerprint, group_id_hash, group_name, tier_label,
                rate_multiplier, rate_source, created_at_remote, last_used_at,
                raw_source, match_status, matched_station_key_id, match_confidence,
                collected_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            rusqlite::params![
                key.id,
                key.station_id,
                key.remote_key_id_hash,
                key.remote_key_name,
                key.api_key_masked,
                key.api_key_fingerprint,
                key.group_id_hash,
                key.group_name,
                key.tier_label,
                key.rate_multiplier,
                key.rate_source,
                key.created_at,
                key.last_used_at,
                key.raw_source,
                key.match_status.as_str(),
                key.matched_station_key_id,
                key.match_confidence,
                key.collected_at,
                now_iso(),
            ],
        )
        .map_err(|error| format!("无法写入远端 Key: {error}"))?;
    Ok(())
}

fn remote_station_key_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RemoteStationKey> {
    let match_status: String = row.get("match_status")?;
    Ok(RemoteStationKey {
        id: row.get("id")?,
        station_id: row.get("station_id")?,
        remote_key_id_hash: row.get("remote_key_id_hash")?,
        remote_key_name: row.get("remote_key_name")?,
        api_key_masked: row.get("api_key_masked")?,
        api_key_fingerprint: row.get("api_key_fingerprint")?,
        group_id_hash: row.get("group_id_hash")?,
        group_name: row.get("group_name")?,
        tier_label: row.get("tier_label")?,
        rate_multiplier: row.get("rate_multiplier")?,
        rate_source: row.get("rate_source")?,
        created_at: row.get("created_at_remote")?,
        last_used_at: row.get("last_used_at")?,
        raw_source: row.get("raw_source")?,
        match_status: RemoteKeyMatchStatus::from_str(&match_status),
        matched_station_key_id: row.get("matched_station_key_id")?,
        match_confidence: row.get("match_confidence")?,
        collected_at: row.get("collected_at")?,
    })
}
```

- [ ] **Step 3: Add database persistence test**

Add a test near other database tests:

```rust
#[test]
fn remote_station_keys_replace_per_station() {
    let database = AppDatabase::new_in_memory_for_tests().expect("database");
    let station_id = "station-remote-test".to_string();
    let key = crate::models::remote_keys::RemoteStationKey {
        id: "remote-key-1".to_string(),
        station_id: station_id.clone(),
        remote_key_id_hash: Some("remote-hash".to_string()),
        remote_key_name: Some("default".to_string()),
        api_key_masked: Some("sk-test****tail".to_string()),
        api_key_fingerprint: None,
        group_id_hash: Some("group-hash".to_string()),
        group_name: Some("vip".to_string()),
        tier_label: None,
        rate_multiplier: Some(0.7),
        rate_source: Some("sub2api_keys".to_string()),
        created_at: None,
        last_used_at: None,
        raw_source: "sub2api_keys".to_string(),
        match_status: crate::models::remote_keys::RemoteKeyMatchStatus::Unbound,
        matched_station_key_id: None,
        match_confidence: 0.0,
        collected_at: now_iso(),
    };
    let saved = database
        .replace_remote_station_keys(station_id.clone(), vec![key])
        .expect("replace remote keys");
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].group_name.as_deref(), Some("vip"));

    let empty = database
        .replace_remote_station_keys(station_id.clone(), Vec::new())
        .expect("replace with empty");
    assert!(empty.is_empty());
}
```

- [ ] **Step 4: Run database test**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml remote_station_keys_replace_per_station
```

Expected: the test passes. If `now_iso()` is private in a different scope, use the existing timestamp helper already available in `database.rs`.

- [ ] **Step 5: Commit Task 2**

```powershell
git add -- src-tauri/src/services/database.rs
git commit -m "feat: persist remote station key discoveries"
```

---

### Task 3: Backend Commands and Capability Dispatch

**Files:**
- Modify: `src-tauri/src/services/remote_keys.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add high-level service functions**

Append to `src-tauri/src/services/remote_keys.rs`:

```rust
use crate::{
    models::{
        remote_keys::{
            BindRemoteStationKeyInput, CreateRemoteStationKeyInput, CreateRemoteStationKeyResult,
            RemoteKeyCapability, RemoteKeyScanResult, RemoteStationKey,
        },
        station_keys::{CreateStationKeyInput, StationKey},
    },
    services::{collectors::adapters, database::AppDatabase},
};

pub fn remote_key_capability(
    database: &AppDatabase,
    station_id: String,
) -> Result<RemoteKeyCapability, String> {
    let station = database.station_for_collector(&station_id)?;
    match station.station_type.as_str() {
        "sub2api" => adapters::sub2api::remote_key_capability(&station),
        "newapi" => adapters::newapi::remote_key_capability(&station),
        station_type => Ok(RemoteKeyCapability {
            station_id,
            station_type: station_type.to_string(),
            can_list_remote_keys: false,
            can_create_remote_key: false,
            can_read_groups: false,
            requires_manual_session: false,
            unsupported_reason: Some("该站点类型暂不支持远端 Key 管理。".to_string()),
        }),
    }
}

pub fn list_remote_keys(database: &AppDatabase, station_id: String) -> Result<Vec<RemoteStationKey>, String> {
    database.list_remote_station_keys(station_id)
}

pub fn scan_remote_keys(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: String,
) -> Result<RemoteKeyScanResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let capability = remote_key_capability(database, station_id.clone())?;
    if !capability.can_list_remote_keys {
        return Ok(RemoteKeyScanResult {
            station_id,
            capability,
            keys: Vec::new(),
            synced_station_key_ids: Vec::new(),
            message: "当前站点暂不支持远端 Key 扫描。".to_string(),
        });
    }
    let keys = match station.station_type.as_str() {
        "sub2api" => adapters::sub2api::scan_remote_keys(database, data_key, &station_id)?,
        "newapi" => adapters::newapi::scan_remote_keys(database, data_key, &station_id)?,
        _ => Vec::new(),
    };
    let saved = database.replace_remote_station_keys(station_id.clone(), keys)?;
    Ok(RemoteKeyScanResult {
        station_id,
        capability,
        synced_station_key_ids: Vec::new(),
        message: format!("已获取 {} 把远端 Key。", saved.len()),
        keys: saved,
    })
}

pub fn create_remote_key(
    database: &AppDatabase,
    data_key: &[u8; 32],
    input: CreateRemoteStationKeyInput,
) -> Result<CreateRemoteStationKeyResult, String> {
    let station = database.station_for_collector(&input.station_id)?;
    let created = match station.station_type.as_str() {
        "sub2api" => adapters::sub2api::create_remote_key(database, data_key, &input)?,
        "newapi" => adapters::newapi::create_remote_key(database, data_key, &input)?,
        _ => return Err("该站点类型暂不支持创建远端 Key。".to_string()),
    };
    let full_key = created
        .full_key_once
        .clone()
        .ok_or_else(|| "远端创建成功，但响应没有返回完整 Key，无法自动保存到本地。".to_string())?;
    let station_key: StationKey = database.create_station_key_with_data_key(
        CreateStationKeyInput {
            station_id: input.station_id.clone(),
            name: input.name.clone(),
            api_key: full_key,
            enabled: true,
            priority: None,
            group_name: input.group_name.clone().or(created.remote_key.group_name.clone()),
            tier_label: created.remote_key.tier_label.clone(),
            group_binding_id: None,
            group_id_hash: input.group_id_hash.clone().or(created.remote_key.group_id_hash.clone()),
            rate_multiplier: created.remote_key.rate_multiplier,
            rate_source: Some("remote_create".to_string()),
            balance_scope: Some("station_key".to_string()),
            note: Some("通过 Relay Pool Desktop 创建的远端 Key".to_string()),
        },
        data_key,
    )?;
    Ok(CreateRemoteStationKeyResult {
        station_key,
        ..created
    })
}

pub fn bind_remote_key(
    database: &AppDatabase,
    input: BindRemoteStationKeyInput,
) -> Result<Vec<RemoteStationKey>, String> {
    database.bind_remote_station_key(input.remote_key_id, input.station_key_id)
}
```

If `bind_remote_station_key` does not exist yet, add it in Task 2's database area before this step:

```rust
pub fn bind_remote_station_key(
    &self,
    remote_key_id: String,
    station_key_id: String,
) -> Result<Vec<RemoteStationKey>, String> {
    let connection = self.connection()?;
    let station_id: String = connection
        .query_row(
            "SELECT station_id FROM station_keys WHERE id = ?1",
            rusqlite::params![station_key_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("无法读取本地 Key 所属站点: {error}"))?;
    connection
        .execute(
            "UPDATE remote_station_keys
             SET match_status = 'matched', matched_station_key_id = ?1, match_confidence = 1.0, updated_at = ?2
             WHERE id = ?3 AND station_id = ?4",
            rusqlite::params![station_key_id, now_iso(), remote_key_id, station_id],
        )
        .map_err(|error| format!("无法绑定远端 Key: {error}"))?;
    list_remote_station_keys_from_connection(&connection, &station_id)
}
```

- [ ] **Step 2: Add Tauri commands**

In `src-tauri/src/commands/mod.rs`, import remote key DTOs:

```rust
use crate::models::remote_keys::{BindRemoteStationKeyInput, CreateRemoteStationKeyInput};
```

Add commands:

```rust
#[tauri::command]
pub fn get_remote_key_capability(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<crate::models::remote_keys::RemoteKeyCapability, String> {
    crate::services::remote_keys::remote_key_capability(&database, station_id)
}

#[tauri::command]
pub fn list_remote_station_keys(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<crate::models::remote_keys::RemoteStationKey>, String> {
    crate::services::remote_keys::list_remote_keys(&database, station_id)
}

#[tauri::command]
pub async fn scan_remote_station_keys(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_id: String,
) -> Result<crate::models::remote_keys::RemoteKeyScanResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        crate::services::remote_keys::scan_remote_keys(&database, &data_key, station_id)
    })
    .await
    .map_err(|error| format!("远端 Key 扫描失败: {error}"))?
}

#[tauri::command]
pub async fn create_remote_station_key(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: CreateRemoteStationKeyInput,
) -> Result<crate::models::remote_keys::CreateRemoteStationKeyResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        crate::services::remote_keys::create_remote_key(&database, &data_key, input)
    })
    .await
    .map_err(|error| format!("创建远端 Key 失败: {error}"))?
}

#[tauri::command]
pub fn bind_remote_station_key(
    database: State<'_, AppDatabase>,
    input: BindRemoteStationKeyInput,
) -> Result<Vec<crate::models::remote_keys::RemoteStationKey>, String> {
    crate::services::remote_keys::bind_remote_key(&database, input)
}
```

- [ ] **Step 3: Register commands**

In `src-tauri/src/lib.rs`, add these entries near Station Key commands:

```rust
commands::get_remote_key_capability,
commands::list_remote_station_keys,
commands::scan_remote_station_keys,
commands::create_remote_station_key,
commands::bind_remote_station_key,
```

- [ ] **Step 4: Run Rust check**

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: compiles or fails only because adapter functions named in this task are not implemented yet. If it fails on missing adapter functions, continue to Task 4 before the next full check.

- [ ] **Step 5: Commit Task 3 after Task 4 compiles**

Do not commit Task 3 until Task 4 provides the adapter stubs. After Task 4 compiles:

```powershell
git add -- src-tauri/src/services/remote_keys.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs src-tauri/src/services/database.rs
git commit -m "feat: expose remote station key commands"
```

---

### Task 4: Sub2API and NewAPI Remote Key Adapter Support

**Files:**
- Modify: `src-tauri/src/services/collectors/adapters/sub2api.rs`
- Modify: `src-tauri/src/services/collectors/adapters/newapi.rs`
- Modify: `src-tauri/src/services/collectors/adapters/mod.rs`

- [ ] **Step 1: Add adapter create result type**

In `src-tauri/src/services/collectors/adapters/mod.rs`, add:

```rust
#[derive(Debug, Clone)]
pub struct CreatedRemoteKey {
    pub remote_key: crate::models::remote_keys::RemoteStationKey,
    pub full_key_once: Option<String>,
    pub message: String,
}
```

- [ ] **Step 2: Add Sub2API capability and scan stubs**

In `src-tauri/src/services/collectors/adapters/sub2api.rs`, add imports:

```rust
use crate::models::{
    remote_keys::{CreateRemoteStationKeyInput, RemoteKeyCapability, RemoteKeyMatchStatus, RemoteStationKey},
    stations::Station,
};
use crate::services::collectors::adapters::CreatedRemoteKey;
use crate::services::remote_keys::api_key_fingerprint;
```

Add functions:

```rust
pub fn remote_key_capability(station: &Station) -> Result<RemoteKeyCapability, String> {
    Ok(RemoteKeyCapability {
        station_id: station.id.clone(),
        station_type: station.station_type.clone(),
        can_list_remote_keys: true,
        can_create_remote_key: true,
        can_read_groups: true,
        requires_manual_session: false,
        unsupported_reason: None,
    })
}

pub fn scan_remote_keys(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_id: &str,
) -> Result<Vec<RemoteStationKey>, String> {
    let station = database.station_for_collector(station_id)?;
    let credentials = database.get_station_credentials(station_id.to_string())?;
    let session = database.resolve_station_session_with_data_key(
        station_id.to_string(),
        data_key,
        crate::services::database::now_millis_for_services() as i64,
    )?;
    let Some(access_token) = session.access_token else {
        return Err("Sub2API 扫描远端 Key 需要 access token。".to_string());
    };
    let urls = collector_base_urls(&station.base_url);
    let url = join_url(&urls.management_base_url, "/api/v1/keys?page=1&page_size=100");
    let payload = get_sub2api_json(&url, &access_token)?;
    let now = crate::services::database::now_iso();
    Ok(parse_remote_key_items(station_id, &payload, &now, credentials.login_username.as_deref()))
}

pub fn create_remote_key(
    database: &AppDatabase,
    data_key: &[u8; 32],
    input: &CreateRemoteStationKeyInput,
) -> Result<CreatedRemoteKey, String> {
    let station = database.station_for_collector(&input.station_id)?;
    let session = database.resolve_station_session_with_data_key(
        input.station_id.clone(),
        data_key,
        crate::services::database::now_millis_for_services() as i64,
    )?;
    let Some(access_token) = session.access_token else {
        return Err("Sub2API 创建远端 Key 需要 access token。".to_string());
    };
    let urls = collector_base_urls(&station.base_url);
    let url = join_url(&urls.management_base_url, "/api/v1/keys");
    let request_body = json!({
        "name": input.name,
        "group": input.group_name,
    });
    let payload = post_sub2api_json(&url, &access_token, &request_body)?;
    let full_key = string_field(&payload, &["key", "api_key", "apiKey", "token"])
        .or_else(|| payload.pointer("/data/key").and_then(Value::as_str).map(ToString::to_string))
        .or_else(|| payload.pointer("/data/api_key").and_then(Value::as_str).map(ToString::to_string));
    let now = crate::services::database::now_iso();
    let remote_key = RemoteStationKey {
        id: crate::services::database::next_id("remote-key"),
        station_id: input.station_id.clone(),
        remote_key_id_hash: string_field(&payload, &["id", "key_id", "keyId"])
            .and_then(|id| api_key_fingerprint(&id)),
        remote_key_name: Some(input.name.clone()),
        api_key_masked: full_key.as_deref().map(crate::services::secrets::mask::mask_secret),
        api_key_fingerprint: full_key.as_deref().and_then(api_key_fingerprint),
        group_id_hash: input.group_id_hash.clone(),
        group_name: input.group_name.clone(),
        tier_label: None,
        rate_multiplier: None,
        rate_source: Some("remote_create".to_string()),
        created_at: Some(now.clone()),
        last_used_at: None,
        raw_source: "sub2api_key_create".to_string(),
        match_status: RemoteKeyMatchStatus::Matched,
        matched_station_key_id: None,
        match_confidence: 1.0,
        collected_at: now,
    };
    Ok(CreatedRemoteKey {
        remote_key,
        full_key_once: full_key,
        message: "远端 Key 已创建。".to_string(),
    })
}
```

- [ ] **Step 3: Add Sub2API parsing helpers and tests**

Append helpers:

```rust
fn get_sub2api_json(url: &str, access_token: &str) -> Result<Value, String> {
    ureq::get(url)
        .set("Authorization", &format!("Bearer {access_token}"))
        .call()
        .map_err(|error| format!("Sub2API 请求失败: {error}"))?
        .into_json::<Value>()
        .map_err(|error| format!("Sub2API 响应不是 JSON: {error}"))
}

fn post_sub2api_json(url: &str, access_token: &str, body: &Value) -> Result<Value, String> {
    ureq::post(url)
        .set("Authorization", &format!("Bearer {access_token}"))
        .send_json(body)
        .map_err(|error| format!("Sub2API 创建 Key 请求失败: {error}"))?
        .into_json::<Value>()
        .map_err(|error| format!("Sub2API 创建 Key 响应不是 JSON: {error}"))
}

fn parse_remote_key_items(
    station_id: &str,
    payload: &Value,
    collected_at: &str,
    _login_hint: Option<&str>,
) -> Vec<RemoteStationKey> {
    remote_key_values(payload)
        .into_iter()
        .map(|item| {
            let full_key = string_field(item, &["key", "api_key", "apiKey", "token"]);
            let masked = full_key
                .as_deref()
                .map(crate::services::secrets::mask::mask_secret)
                .or_else(|| string_field(item, &["key_masked", "api_key_masked", "apiKeyMasked", "masked_key"]));
            let remote_id = string_field(item, &["id", "key_id", "keyId"]);
            RemoteStationKey {
                id: crate::services::database::next_id("remote-key"),
                station_id: station_id.to_string(),
                remote_key_id_hash: remote_id.as_deref().and_then(api_key_fingerprint),
                remote_key_name: string_field(item, &["name", "label", "remark", "description"]),
                api_key_masked: masked,
                api_key_fingerprint: full_key.as_deref().and_then(api_key_fingerprint),
                group_id_hash: string_field(item, &["group_id", "groupId", "group"]).and_then(|value| api_key_fingerprint(&value)),
                group_name: string_field(item, &["group_name", "groupName", "group"]),
                tier_label: string_field(item, &["tier", "tier_label", "tierLabel"]),
                rate_multiplier: numeric_field(item, &["rate_multiplier", "rateMultiplier", "ratio", "rate"]),
                rate_source: Some("sub2api_keys".to_string()),
                created_at: string_field(item, &["created_at", "createdAt"]),
                last_used_at: string_field(item, &["last_used_at", "lastUsedAt"]),
                raw_source: "sub2api_keys".to_string(),
                match_status: RemoteKeyMatchStatus::Unbound,
                matched_station_key_id: None,
                match_confidence: 0.0,
                collected_at: collected_at.to_string(),
            }
        })
        .collect()
}

fn remote_key_values(payload: &Value) -> Vec<&Value> {
    if let Some(items) = payload.as_array() {
        return items.iter().collect();
    }
    for key in ["data", "items", "list", "keys", "records"] {
        if let Some(value) = payload.get(key).and_then(Value::as_array) {
            return value.iter().collect();
        }
    }
    Vec::new()
}
```

Add tests:

```rust
#[test]
fn parses_sub2api_remote_key_payload() {
    let payload = json!({
        "data": [
            {
                "id": "remote-1",
                "name": "codex",
                "key": "sk-live-abc-tail",
                "group": "vip",
                "rate_multiplier": 0.6
            }
        ]
    });
    let rows = parse_remote_key_items("station-1", &payload, "2026-07-05T00:00:00Z", None);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].remote_key_name.as_deref(), Some("codex"));
    assert_eq!(rows[0].group_name.as_deref(), Some("vip"));
    assert_eq!(rows[0].rate_multiplier, Some(0.6));
    assert!(rows[0].api_key_fingerprint.is_some());
}
```

- [ ] **Step 4: Add NewAPI capability fallback**

In `src-tauri/src/services/collectors/adapters/newapi.rs`, add imports and functions:

```rust
use crate::models::{
    remote_keys::{CreateRemoteStationKeyInput, RemoteKeyCapability, RemoteStationKey},
    stations::Station,
};
use crate::services::collectors::adapters::CreatedRemoteKey;

pub fn remote_key_capability(station: &Station) -> Result<RemoteKeyCapability, String> {
    Ok(RemoteKeyCapability {
        station_id: station.id.clone(),
        station_type: station.station_type.clone(),
        can_list_remote_keys: false,
        can_create_remote_key: false,
        can_read_groups: true,
        requires_manual_session: true,
        unsupported_reason: Some("当前 NewAPI 适配器只确认分组采集，暂未确认该站点的远端 Key 管理接口。".to_string()),
    })
}

pub fn scan_remote_keys(
    _database: &AppDatabase,
    _data_key: &[u8; 32],
    _station_id: &str,
) -> Result<Vec<RemoteStationKey>, String> {
    Ok(Vec::new())
}

pub fn create_remote_key(
    _database: &AppDatabase,
    _data_key: &[u8; 32],
    _input: &CreateRemoteStationKeyInput,
) -> Result<CreatedRemoteKey, String> {
    Err("当前 NewAPI 站点暂未确认远端 Key 创建接口。".to_string())
}
```

- [ ] **Step 5: Run adapter tests and check**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml parses_sub2api_remote_key_payload
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: parser test passes and `cargo check` passes. Fix type visibility if `now_iso` or `next_id` is private; make small public helpers in `database.rs` only if no existing public helper exists.

- [ ] **Step 6: Commit Task 4 with Task 3 if needed**

If Task 3 was not committed because adapters were missing:

```powershell
git add -- src-tauri/src/services/collectors/adapters/mod.rs src-tauri/src/services/collectors/adapters/sub2api.rs src-tauri/src/services/collectors/adapters/newapi.rs src-tauri/src/services/remote_keys.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "feat: add remote key adapter commands"
```

---

### Task 5: Frontend Types and API Wrappers

**Files:**
- Modify: `src/lib/types/stationKeys.ts`
- Modify: `src/lib/api/stationKeys.ts`

- [ ] **Step 1: Add TypeScript types**

Append to `src/lib/types/stationKeys.ts`:

```typescript
export type RemoteKeyMatchStatus = "matched" | "possible" | "unbound";

export type RemoteKeyCapability = {
  stationId: string;
  stationType: string;
  canListRemoteKeys: boolean;
  canCreateRemoteKey: boolean;
  canReadGroups: boolean;
  requiresManualSession: boolean;
  unsupportedReason: string | null;
};

export type RemoteStationKey = {
  id: string;
  stationId: string;
  remoteKeyIdHash: string | null;
  remoteKeyName: string | null;
  apiKeyMasked: string | null;
  apiKeyFingerprint: string | null;
  groupIdHash: string | null;
  groupName: string | null;
  tierLabel: string | null;
  rateMultiplier: number | null;
  rateSource: string | null;
  createdAt: string | null;
  lastUsedAt: string | null;
  rawSource: string;
  matchStatus: RemoteKeyMatchStatus;
  matchedStationKeyId: string | null;
  matchConfidence: number;
  collectedAt: string;
};

export type RemoteKeyScanResult = {
  stationId: string;
  capability: RemoteKeyCapability;
  keys: RemoteStationKey[];
  syncedStationKeyIds: string[];
  message: string;
};

export type CreateRemoteStationKeyInput = {
  stationId: string;
  name: string;
  groupIdHash: string | null;
  groupName: string | null;
};

export type CreateRemoteStationKeyResult = {
  remoteKey: RemoteStationKey;
  stationKey: StationKey;
  fullKeyOnce: string | null;
  message: string;
};
```

- [ ] **Step 2: Add API wrappers with memory fallback**

Update imports in `src/lib/api/stationKeys.ts` to include the new types. Add memory state:

```typescript
const memoryRemoteKeys = new Map<string, RemoteStationKey[]>();
```

Add wrappers:

```typescript
export function getRemoteKeyCapability(stationId: string) {
  return invoke<RemoteKeyCapability>("get_remote_key_capability", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return {
        stationId,
        stationType: "sub2api",
        canListRemoteKeys: true,
        canCreateRemoteKey: true,
        canReadGroups: true,
        requiresManualSession: false,
        unsupportedReason: null,
      };
    }
    throw error;
  });
}

export function listRemoteStationKeys(stationId: string) {
  return invoke<RemoteStationKey[]>("list_remote_station_keys", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memoryRemoteKeys.get(stationId) ?? [];
    }
    throw error;
  });
}

export function scanRemoteStationKeys(stationId: string) {
  return invoke<RemoteKeyScanResult>("scan_remote_station_keys", { stationId }).catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      const capability = await getRemoteKeyCapability(stationId);
      const keys = memoryRemoteKeys.get(stationId) ?? [];
      return {
        stationId,
        capability,
        keys,
        syncedStationKeyIds: [],
        message: keys.length ? `浏览器预览模式：显示 ${keys.length} 把模拟远端 Key。` : "浏览器预览模式：暂无模拟远端 Key。",
      };
    }
    throw error;
  });
}

export function createRemoteStationKey(input: CreateRemoteStationKeyInput) {
  return invoke<CreateRemoteStationKeyResult>("create_remote_station_key", { input }).catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      const stationKey = await createStationKey({
        stationId: input.stationId,
        name: input.name,
        apiKey: `sk-preview-${Date.now()}`,
        enabled: true,
        priority: null,
        groupName: input.groupName,
        groupIdHash: input.groupIdHash,
        tierLabel: null,
        rateMultiplier: null,
        rateSource: "remote_create",
        balanceScope: "station_key",
        note: "浏览器预览模式创建",
      });
      const remoteKey: RemoteStationKey = {
        id: `remote-${Date.now()}`,
        stationId: input.stationId,
        remoteKeyIdHash: null,
        remoteKeyName: input.name,
        apiKeyMasked: "sk-preview****mock",
        apiKeyFingerprint: null,
        groupIdHash: input.groupIdHash,
        groupName: input.groupName,
        tierLabel: null,
        rateMultiplier: null,
        rateSource: "remote_create",
        createdAt: new Date().toISOString(),
        lastUsedAt: null,
        rawSource: "memory",
        matchStatus: "matched",
        matchedStationKeyId: stationKey.id,
        matchConfidence: 1,
        collectedAt: new Date().toISOString(),
      };
      memoryRemoteKeys.set(input.stationId, [remoteKey, ...(memoryRemoteKeys.get(input.stationId) ?? [])]);
      return {
        remoteKey,
        stationKey,
        fullKeyOnce: null,
        message: "浏览器预览模式：远端 Key 已模拟创建。",
      };
    }
    throw error;
  });
}

export function bindRemoteStationKey(remoteKeyId: string, stationKeyId: string) {
  return invoke<RemoteStationKey[]>("bind_remote_station_key", {
    input: { remoteKeyId, stationKeyId },
  }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      for (const [stationId, keys] of memoryRemoteKeys) {
        const next = keys.map((key) =>
          key.id === remoteKeyId
            ? { ...key, matchStatus: "matched" as const, matchedStationKeyId: stationKeyId, matchConfidence: 1 }
            : key,
        );
        memoryRemoteKeys.set(stationId, next);
      }
      return Array.from(memoryRemoteKeys.values()).flat();
    }
    throw error;
  });
}
```

- [ ] **Step 3: Run TypeScript check**

```powershell
pnpm.cmd exec tsc --noEmit
```

Expected: type errors only if import ordering or exported names are missing. Fix imports and rerun until clean.

- [ ] **Step 4: Commit Task 5**

```powershell
git add -- src/lib/types/stationKeys.ts src/lib/api/stationKeys.ts
git commit -m "feat: add remote station key client API"
```

---

### Task 6: Supplier Form Local Key Row Editor

**Files:**
- Create: `src/features/stations/components/StationKeyRowsEditor.tsx`
- Modify: `src/features/stations/AddProviderPage.tsx`

- [ ] **Step 1: Create focused key row editor**

Create `src/features/stations/components/StationKeyRowsEditor.tsx`:

```tsx
import { Plus, Trash2 } from "lucide-react";
import { Button, SwitchControl } from "@/components/ui";
import { cn } from "@/lib/utils";

export type StationKeyDraft = {
  clientId: string;
  id: string | null;
  name: string;
  apiKey: string;
  groupName: string;
  rateMultiplier: string;
  enabled: boolean;
  note: string;
  deleteRequested: boolean;
};

type StationKeyRowsEditorProps = {
  rows: StationKeyDraft[];
  disabled?: boolean;
  onRowsChange: (rows: StationKeyDraft[]) => void;
};

const inputClassName =
  "h-8 min-w-0 rounded-[var(--surface-radius)] border border-border bg-white px-2.5 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]";

export function createEmptyStationKeyDraft(index: number): StationKeyDraft {
  return {
    clientId: `draft-${Date.now()}-${index}`,
    id: null,
    name: `Key ${index + 1}`,
    apiKey: "",
    groupName: "",
    rateMultiplier: "",
    enabled: true,
    note: "",
    deleteRequested: false,
  };
}

export function StationKeyRowsEditor({ rows, disabled, onRowsChange }: StationKeyRowsEditorProps) {
  const visibleRows = rows.filter((row) => !row.deleteRequested);
  function updateRow(clientId: string, patch: Partial<StationKeyDraft>) {
    onRowsChange(rows.map((row) => (row.clientId === clientId ? { ...row, ...patch } : row)));
  }
  function removeRow(clientId: string) {
    onRowsChange(
      rows.flatMap((row) => {
        if (row.clientId !== clientId) return [row];
        return row.id ? [{ ...row, deleteRequested: true }] : [];
      }),
    );
  }
  return (
    <div className="grid gap-2">
      <div className="grid grid-cols-[minmax(7rem,1fr)_minmax(10rem,1.4fr)_minmax(6rem,0.8fr)_5rem_5rem_2rem] gap-2 px-1 text-[11px] font-medium text-slate-500">
        <span>名称</span>
        <span>密钥</span>
        <span>分组</span>
        <span>倍率</span>
        <span>启用</span>
        <span />
      </div>
      {visibleRows.map((row) => (
        <div
          key={row.clientId}
          className={cn(
            "grid grid-cols-[minmax(7rem,1fr)_minmax(10rem,1.4fr)_minmax(6rem,0.8fr)_5rem_5rem_2rem] items-center gap-2",
            disabled && "opacity-70",
          )}
        >
          <input className={inputClassName} value={row.name} disabled={disabled} onChange={(event) => updateRow(row.clientId, { name: event.target.value })} />
          <input
            className={inputClassName}
            type="password"
            value={row.apiKey}
            disabled={disabled}
            placeholder={row.id ? "留空保留旧密钥" : "sk-..."}
            onChange={(event) => updateRow(row.clientId, { apiKey: event.target.value })}
          />
          <input className={inputClassName} value={row.groupName} disabled={disabled} onChange={(event) => updateRow(row.clientId, { groupName: event.target.value })} />
          <input className={inputClassName} type="number" step="0.01" min="0" value={row.rateMultiplier} disabled={disabled} onChange={(event) => updateRow(row.clientId, { rateMultiplier: event.target.value })} />
          <SwitchControl ariaLabel="启用密钥" checked={row.enabled} disabled={disabled} onCheckedChange={() => updateRow(row.clientId, { enabled: !row.enabled })} />
          <Button type="button" variant="ghost" size="icon" disabled={disabled} onClick={() => removeRow(row.clientId)}>
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      ))}
      <div>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled={disabled}
          onClick={() => onRowsChange([...rows, createEmptyStationKeyDraft(rows.length)])}
        >
          <Plus className="h-3.5 w-3.5" />
          添加密钥
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Load and save key drafts in AddProviderPage**

Update imports in `src/features/stations/AddProviderPage.tsx`:

```tsx
import {
  createStationKey,
  deleteStationKey,
  getStationCredentials,
  listStationKeys,
  updateStationCredentials,
  updateStationKey,
} from "@/lib/api/stationKeys";
import type { StationCredentials, StationKey } from "@/lib/types/stationKeys";
import { createEmptyStationKeyDraft, StationKeyRowsEditor, type StationKeyDraft } from "./components/StationKeyRowsEditor";
```

Add state:

```tsx
const [keyRows, setKeyRows] = useState<StationKeyDraft[]>([createEmptyStationKeyDraft(0)]);
```

In the edit `Promise.all`, include `listStationKeys(stationId)` and set rows:

```tsx
void Promise.all([listStations(), getStationCredentials(stationId), listStationKeys(stationId)])
  .then(([stations, credentials, keys]) => {
    // existing station lookup
    setKeyRows(keys.length ? keys.map(keyToDraft) : []);
  })
```

Add helpers near the bottom:

```tsx
function keyToDraft(key: StationKey): StationKeyDraft {
  return {
    clientId: key.id,
    id: key.id,
    name: key.name,
    apiKey: "",
    groupName: key.groupName ?? "",
    rateMultiplier: key.rateMultiplier === null ? "" : String(key.rateMultiplier),
    enabled: key.enabled,
    note: key.note ?? "",
    deleteRequested: false,
  };
}

async function saveKeyRows(targetStationId: string, rows: StationKeyDraft[]) {
  const visibleRows = rows.filter((row) => !row.deleteRequested);
  for (const row of rows.filter((item) => item.deleteRequested && item.id)) {
    await deleteStationKey(row.id!);
  }
  for (const [index, row] of visibleRows.entries()) {
    if (!row.name.trim()) {
      throw new Error("密钥名称不能为空。");
    }
    const rateMultiplier = row.rateMultiplier.trim() ? Number(row.rateMultiplier) : null;
    if (row.rateMultiplier.trim() && !Number.isFinite(rateMultiplier)) {
      throw new Error(`密钥 ${row.name} 的倍率不是有效数字。`);
    }
    if (row.id) {
      await updateStationKey({
        id: row.id,
        stationId: targetStationId,
        name: row.name.trim(),
        apiKey: row.apiKey.trim() ? row.apiKey.trim() : null,
        enabled: row.enabled,
        priority: index,
        groupName: row.groupName.trim() || null,
        tierLabel: null,
        groupBindingId: null,
        groupIdHash: null,
        rateMultiplier,
        rateSource: rateMultiplier === null ? null : "manual",
        balanceScope: "station_key",
        status: "unchecked",
        note: row.note.trim() || null,
      });
    } else if (row.apiKey.trim()) {
      await createStationKey({
        stationId: targetStationId,
        name: row.name.trim(),
        apiKey: row.apiKey.trim(),
        enabled: row.enabled,
        priority: index,
        groupName: row.groupName.trim() || null,
        tierLabel: null,
        groupBindingId: null,
        groupIdHash: null,
        rateMultiplier,
        rateSource: rateMultiplier === null ? null : "manual",
        balanceScope: "station_key",
        note: row.note.trim() || null,
      });
    }
  }
}
```

In `handleSubmit`, remove the `!editing && !form.apiKey.trim()` requirement or replace it with a check that at least one local key exists:

```tsx
const hasAnyNewKey = keyRows.some((row) => !row.deleteRequested && row.apiKey.trim());
if (!editing && !form.apiKey.trim() && !hasAnyNewKey) {
  toast.info("请至少填写一把密钥");
  return;
}
```

After `createStation` or `updateStation`, call:

```tsx
await saveKeyRows(targetStationId, keyRows);
```

- [ ] **Step 3: Render key section**

Inside the `PageForm` section after `连接信息` and before `可选项`, add:

```tsx
<SectionCard title="密钥">
  <StationKeyRowsEditor rows={keyRows} disabled={saving || loading} onRowsChange={setKeyRows} />
</SectionCard>
```

- [ ] **Step 4: Run frontend check**

```powershell
pnpm.cmd exec tsc --noEmit
```

Expected: TypeScript passes. If `SwitchControl` uses `onChange` rather than `onCheckedChange`, inspect `src/components/ui/SwitchControl.tsx` and use the existing prop name.

- [ ] **Step 5: Commit Task 6**

```powershell
git add -- src/features/stations/AddProviderPage.tsx src/features/stations/components/StationKeyRowsEditor.tsx
git commit -m "feat: edit station keys in supplier form"
```

---

### Task 7: Remote Discovery List and Create Dialog UI

**Files:**
- Create: `src/features/stations/components/RemoteKeyDiscoveryList.tsx`
- Create: `src/features/stations/components/CreateRemoteKeyDialog.tsx`
- Modify: `src/features/stations/AddProviderPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`

- [ ] **Step 1: Create remote discovery list component**

Create `src/features/stations/components/RemoteKeyDiscoveryList.tsx`:

```tsx
import { Link2 } from "lucide-react";
import { Button, StatusBadge } from "@/components/ui";
import type { RemoteStationKey, StationKey } from "@/lib/types/stationKeys";

type RemoteKeyDiscoveryListProps = {
  keys: RemoteStationKey[];
  localKeys: StationKey[];
  loading?: boolean;
  onBind: (remoteKeyId: string, stationKeyId: string) => void;
};

const statusLabel = {
  matched: "已匹配",
  possible: "可能匹配",
  unbound: "未绑定",
} as const;

export function RemoteKeyDiscoveryList({ keys, localKeys, loading, onBind }: RemoteKeyDiscoveryListProps) {
  if (!keys.length) {
    return (
      <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-slate-50 px-3 py-2 text-sm text-muted-foreground">
        {loading ? "正在获取远端 Key..." : "暂无远端发现。"}
      </div>
    );
  }
  return (
    <div className="grid gap-2">
      {keys.map((key) => {
        const matched = localKeys.find((item) => item.id === key.matchedStationKeyId);
        return (
          <div key={key.id} className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2">
            <div className="min-w-0">
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                <span className="truncate text-sm font-medium text-slate-800">{key.remoteKeyName ?? "未命名远端 Key"}</span>
                <StatusBadge tone={key.matchStatus === "matched" ? "healthy" : key.matchStatus === "possible" ? "warning" : "disabled"}>
                  {statusLabel[key.matchStatus]}
                </StatusBadge>
              </div>
              <div className="mt-1 flex min-w-0 flex-wrap gap-2 text-xs text-muted-foreground">
                <span className="font-mono">{key.apiKeyMasked ?? "未返回脱敏 Key"}</span>
                <span>{key.groupName ?? "未分组"}</span>
                <span>{key.rateMultiplier === null ? "倍率未知" : `${key.rateMultiplier}x`}</span>
                {matched && <span>本地：{matched.name}</span>}
              </div>
            </div>
            {key.matchStatus !== "matched" && localKeys.length > 0 && (
              <Button type="button" variant="outline" size="sm" onClick={() => onBind(key.id, localKeys[0].id)}>
                <Link2 className="h-3.5 w-3.5" />
                绑定
              </Button>
            )}
          </div>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 2: Create remote key dialog**

Create `src/features/stations/components/CreateRemoteKeyDialog.tsx`:

```tsx
import { useState, type FormEvent } from "react";
import { KeyRound } from "lucide-react";
import { Button, Dialog, SelectControl } from "@/components/ui";

type CreateRemoteKeyDialogProps = {
  open: boolean;
  groups: Array<{ groupIdHash: string | null; groupName: string }>;
  saving?: boolean;
  onClose: () => void;
  onSubmit: (input: { name: string; groupIdHash: string | null; groupName: string | null }) => void;
};

const inputClassName =
  "h-8 rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.18)]";

export function CreateRemoteKeyDialog({ open, groups, saving, onClose, onSubmit }: CreateRemoteKeyDialogProps) {
  const [name, setName] = useState("Relay Pool Key");
  const [groupName, setGroupName] = useState(groups[0]?.groupName ?? "");
  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    const group = groups.find((item) => item.groupName === groupName) ?? null;
    onSubmit({
      name,
      groupIdHash: group?.groupIdHash ?? null,
      groupName: group?.groupName ?? groupName || null,
    });
  }
  return (
    <Dialog
      open={open}
      title="新建远端 Key"
      description="选择站点分组并在网站上创建一把新 Key，成功后会保存到本地并启用。"
      onClose={onClose}
      footer={
        <>
          <Button type="button" variant="secondary" onClick={onClose} disabled={saving}>取消</Button>
          <Button type="submit" form="create-remote-key-form" disabled={saving}>
            <KeyRound className="h-3.5 w-3.5" />
            {saving ? "创建中" : "创建"}
          </Button>
        </>
      }
    >
      <form id="create-remote-key-form" className="grid gap-3 p-5" onSubmit={handleSubmit}>
        <label className="grid gap-1.5 text-xs font-medium text-slate-600">
          名称
          <input className={inputClassName} value={name} onChange={(event) => setName(event.target.value)} required />
        </label>
        <label className="grid gap-1.5 text-xs font-medium text-slate-600">
          分组
          <SelectControl
            ariaLabel="远端 Key 分组"
            className={inputClassName}
            value={groupName}
            options={groups.map((group) => ({ value: group.groupName, label: group.groupName }))}
            onChange={setGroupName}
          />
        </label>
      </form>
    </Dialog>
  );
}
```

- [ ] **Step 3: Wire remote actions in AddProviderPage**

Update `AddProviderPage` imports:

```tsx
import {
  bindRemoteStationKey,
  createRemoteStationKey,
  getRemoteKeyCapability,
  listRemoteStationKeys,
  scanRemoteStationKeys,
} from "@/lib/api/stationKeys";
import type { RemoteKeyCapability, RemoteStationKey } from "@/lib/types/stationKeys";
import { CreateRemoteKeyDialog } from "./components/CreateRemoteKeyDialog";
import { RemoteKeyDiscoveryList } from "./components/RemoteKeyDiscoveryList";
```

Add state:

```tsx
const [remoteCapability, setRemoteCapability] = useState<RemoteKeyCapability | null>(null);
const [remoteKeys, setRemoteKeys] = useState<RemoteStationKey[]>([]);
const [localStationKeys, setLocalStationKeys] = useState<StationKey[]>([]);
const [remoteLoading, setRemoteLoading] = useState(false);
const [createRemoteOpen, setCreateRemoteOpen] = useState(false);
```

When editing, load capability, previous discoveries, and local Station Keys:

```tsx
if (stationId) {
  void Promise.all([getRemoteKeyCapability(stationId), listRemoteStationKeys(stationId), listStationKeys(stationId)])
    .then(([capability, discoveries, keys]) => {
      if (!alive) return;
      setRemoteCapability(capability);
      setRemoteKeys(discoveries);
      setLocalStationKeys(keys);
    })
    .catch((requestError) => toast.error("读取远端 Key 状态失败", readError(requestError)));
}
```

Add handlers:

```tsx
async function handleScanRemoteKeys() {
  if (!stationId) {
    toast.info("请先保存供应商后再获取远端 Key");
    return;
  }
  setRemoteLoading(true);
  try {
    const result = await scanRemoteStationKeys(stationId);
    setRemoteCapability(result.capability);
    setRemoteKeys(result.keys);
    toast.success("远端 Key 已获取", result.message);
  } catch (requestError) {
    toast.error("获取远端 Key 失败", readError(requestError));
  } finally {
    setRemoteLoading(false);
  }
}

async function handleCreateRemoteKey(input: { name: string; groupIdHash: string | null; groupName: string | null }) {
  if (!stationId) {
    toast.info("请先保存供应商后再创建远端 Key");
    return;
  }
  setRemoteLoading(true);
  try {
    const result = await createRemoteStationKey({ stationId, ...input });
    setRemoteKeys((current) => [result.remoteKey, ...current.filter((key) => key.id !== result.remoteKey.id)]);
    const keys = await listStationKeys(stationId);
    setLocalStationKeys(keys);
    setKeyRows(keys.map(keyToDraft));
    setCreateRemoteOpen(false);
    toast.success("远端 Key 已创建", "已保存到本地并启用。");
  } catch (requestError) {
    toast.error("创建远端 Key 失败", readError(requestError));
  } finally {
    setRemoteLoading(false);
  }
}

async function handleBindRemoteKey(remoteKeyId: string, stationKeyId: string) {
  try {
    const next = await bindRemoteStationKey(remoteKeyId, stationKeyId);
    setRemoteKeys(next.filter((key) => key.stationId === stationId));
    toast.success("远端 Key 已绑定");
  } catch (requestError) {
    toast.error("绑定远端 Key 失败", readError(requestError));
  }
}
```

Add header actions to the `密钥` section:

```tsx
<SectionCard
  title="密钥"
  action={
    <>
      <Button type="button" variant="secondary" size="sm" disabled={!editing || remoteLoading || remoteCapability?.canListRemoteKeys === false} onClick={handleScanRemoteKeys}>
        获取所有 Key
      </Button>
      <Button type="button" variant="secondary" size="sm" disabled={!editing || remoteLoading || remoteCapability?.canCreateRemoteKey === false} onClick={() => setCreateRemoteOpen(true)}>
        新建远端 Key
      </Button>
    </>
  }
>
  <StationKeyRowsEditor rows={keyRows} disabled={saving || loading} onRowsChange={setKeyRows} />
  {editing && (
    <div className="mt-3">
      <RemoteKeyDiscoveryList
        keys={remoteKeys}
        localKeys={localStationKeys}
        loading={remoteLoading}
        onBind={handleBindRemoteKey}
      />
    </div>
  )}
</SectionCard>
```

After the form, render:

```tsx
<CreateRemoteKeyDialog
  open={createRemoteOpen}
  groups={remoteKeys
    .filter((key) => key.groupName)
    .map((key) => ({ groupIdHash: key.groupIdHash, groupName: key.groupName! }))}
  saving={remoteLoading}
  onClose={() => setCreateRemoteOpen(false)}
  onSubmit={handleCreateRemoteKey}
/>
```

- [ ] **Step 4: Add supplier row create action if current StationsPage owns row actions**

In `src/features/stations/StationsPage.tsx`, locate the row action group that already includes edit/open controls. Add a compact button:

```tsx
<Button variant="outline" size="sm" onClick={() => onEditProvider?.(row.station.id)}>
  <KeyRound className="h-3.5 w-3.5" />
  Key
</Button>
```

If `StationsPage` no longer owns supplier-row actions in the active branch, skip this edit and keep the create action in `AddProviderPage`; note the skip in the final task report.

- [ ] **Step 5: Run frontend build**

```powershell
pnpm.cmd build
```

Expected: TypeScript and Vite build pass. `SectionCard` uses the singular `action` prop, so do not add a new shared component API for these buttons.

- [ ] **Step 6: Commit Task 7**

```powershell
git add -- src/features/stations/AddProviderPage.tsx src/features/stations/StationsPage.tsx src/features/stations/components/RemoteKeyDiscoveryList.tsx src/features/stations/components/CreateRemoteKeyDialog.tsx
git commit -m "feat: add remote key discovery UI"
```

---

### Task 8: Final Verification and Integration Sweep

**Files:**
- Read/verify only unless a previous task left a compile failure.

- [ ] **Step 1: Run Rust tests**

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml
```

Expected: all Rust tests pass. If existing unrelated tests fail, capture the exact failing test names and determine whether they involve files changed by this plan.

- [ ] **Step 2: Run Cargo check**

```powershell
cargo check --manifest-path .\src-tauri\Cargo.toml
```

Expected: check passes.

- [ ] **Step 3: Run frontend build**

```powershell
pnpm.cmd build
```

Expected: `tsc --noEmit` and `vite build` pass.

- [ ] **Step 4: Manual smoke with dev app**

Start:

```powershell
pnpm.cmd tauri:dev
```

Expected:

- App opens at the local Tauri window.
- Add supplier page shows the new `密钥` row editor.
- Clicking `添加密钥` appends one row without layout shift.
- Editing an existing supplier shows existing Station Keys in rows.
- Leaving an existing key field empty keeps the old secret.
- `获取所有 Key` is disabled or reports unsupported for stations without capability.
- `新建远端 Key` is disabled or reports unsupported for NewAPI fallback deployments.
- Browser preview still works through memory fallback if Tauri invoke is unavailable.

- [ ] **Step 5: Check exact git scope**

```powershell
git status --short
git diff --stat
```

Expected: only files from this plan are modified. If unrelated pre-existing changes are present, do not stage them.

- [ ] **Step 6: Final commit if verification fixes were needed**

Only if Task 8 required fixes:

```powershell
git add -- <exact changed files>
git commit -m "fix: verify remote station key management"
```

---

## Self-Review Notes

- Spec coverage: the plan covers remote capability detection, remote scan without auto-import, match/sync primitives, remote create with local enabled save, supplier-form key rows, Sub2API concrete support, NewAPI capability fallback, persistence, and verification.
- Scope boundary: remote deletion and a full sync center are excluded.
- Security boundary: full keys are never logged or shown after routine save; create response full key is only carried long enough to save locally.
- Known implementation sensitivity: `database.rs` may already have timestamp/id helpers with private visibility. Prefer reusing existing helpers; if they are private to the file, keep the new database code in the same file or expose only tiny service-safe wrappers.
