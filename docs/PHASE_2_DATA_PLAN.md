# Phase 2 Data Plan

Phase 2 introduces the first local persistence layer for Relay Pool Desktop. The scope is intentionally narrow: SQLite initialization, `stations` CRUD, basic `settings` persistence, Tauri commands, and frontend wiring for the Stations and Settings pages.

## Implemented Scope

- Initializes a local SQLite database at app startup.
- Creates `stations` and `settings` tables if they do not exist.
- Seeds default settings on first run.
- Stores station basics in SQLite.
- Wires the Stations page to local data instead of `src/lib/mock/stations.ts`.
- Wires part of the Settings page to local persisted settings.
- Keeps Dashboard, Collectors, Pricing, Routing, and Logs on Phase 1 mock data.

## SQLite Choice

The Rust side uses `rusqlite` with the `bundled` feature.

Reasoning:

- It is lightweight and stable for local desktop CRUD.
- It avoids introducing an async database runtime before the app needs one.
- It keeps Phase 2 easy to reason about with a single guarded SQLite connection.
- The bundled SQLite feature makes the project less dependent on system SQLite availability.

## Database Location

The database file is created through Tauri's app data directory API:

```txt
<app data dir>/relay-pool-desktop.sqlite3
```

On Windows this is expected to resolve under the user's local application data area for the app identifier, not inside the repository. The exact path is also returned to the Settings page as read-only information.

The repository `.gitignore` already excludes common local database files:

- `*.db`
- `*.db-shm`
- `*.db-wal`
- `*.sqlite`
- `*.sqlite3`
- `data/`
- `local-data/`

## Tables

### `stations`

Fields:

- `id`
- `name`
- `station_type`
- `base_url`
- `api_key`
- `enabled`
- `priority`
- `credit_per_cny`
- `balance_raw`
- `balance_cny`
- `low_balance_threshold_cny`
- `status`
- `latency_ms`
- `last_checked_at`
- `last_pricing_fetched_at`
- `note`
- `created_at`
- `updated_at`

Deletion behavior: Phase 2 uses hard delete. After delete, station priorities are normalized.

### `settings`

The settings table is key-value based:

- `key`
- `value`
- `updated_at`

Seeded keys:

- `local_proxy_port`
- `local_key`
- `default_routing_strategy`
- `low_balance_threshold_cny`
- `collector_interval_minutes`
- `tray_behavior`

## API Key Storage Limitation

Phase 2 stores station `api_key` and `local_key` in the local SQLite database as plaintext.

This is acceptable only as an early local-data milestone. Before Phase 3 / Phase 4 handles real station login, real proxy requests, or real user keys, keys must move to local encryption or the operating system keychain. The frontend currently displays only masked keys by default and does not return full station keys from list/create/update commands.

## Tauri Commands

Implemented commands:

- `list_stations`
- `create_station`
- `update_station`
- `delete_station`
- `reorder_stations`
- `get_settings`
- `update_settings`

Command behavior:

- Parameters and return values use typed Rust structs with serde camelCase mapping for the frontend.
- Errors are returned as readable strings.
- Station list results include `apiKeyMasked` and `apiKeyPresent`, not the full `api_key`.
- Editing a station can keep the existing key by submitting no replacement key.

## Frontend Wiring

Added frontend API wrappers:

- `src/lib/api/stations.ts`
- `src/lib/api/settings.ts`

Added frontend types:

- `src/lib/types/stations.ts`
- `src/lib/types/settings.ts`

Stations page persisted operations:

- Load stations from SQLite.
- Empty state when no stations exist.
- Add station.
- Add example station.
- Edit station basics.
- Delete station.
- Enable / disable station.
- Move station up / down and persist priority.

Settings page persisted fields:

- Local proxy port.
- Default routing strategy.
- Low balance threshold.
- Collector interval.
- Tray behavior placeholder.

## Not Included In Phase 2

- No local OpenAI-compatible proxy.
- No request forwarding.
- No Sub2API real collection.
- No NewAPI collection.
- No real health checks.
- No pricing snapshots.
- No request logs database table.
- No encrypted key storage yet.
- No cloud sync.
- No account, payment, team, or SaaS behavior.

## Next Stage Suggestions

Recommended next steps:

1. Add encrypted local key storage before any real upstream requests.
2. Add station edit form support for explicit key rotation and key clearing.
3. Add pricing snapshot tables before implementing collectors.
4. Add request log and health check tables before implementing proxy fallback.
5. Add automated command-level tests around schema initialization and station CRUD.
