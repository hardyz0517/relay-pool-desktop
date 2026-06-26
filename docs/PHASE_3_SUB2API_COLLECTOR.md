# Phase 3 Collector Prototype

Phase 3 builds the first real collector loop without implementing proxy, routing, or full WebView login capture.

## Goals

- Treat a Station as one relay website plus one user login account.
- Support multiple API keys under one Station.
- Persist login account metadata, station keys, and collector snapshots in SQLite.
- Provide Tauri commands for key management, credentials, and station information detect / collect.
- Show real collector snapshots in the collection page instead of pure mock data.
- Keep Phase 2 stations/settings persistence intact.

## P3.1 Productization

P3.1 renames the page from `Sub2API 采集` to `信息采集`, because the page will later host Sub2API, NewAPI, OpenAI-compatible, and custom station collectors in one console.

The user-facing page now shows采集结论、识别类型、最近采集时间、识别结果、接口探测结果和历史快照. Raw snapshot / internal JSON is no longer part of the main UI. It is available only in a default-collapsed developer details section, uses redacted data, and is truncated before display/copy.

## Product Model

### Station

A Station represents:

```txt
one relay website + one user login account
```

If a user has two different accounts on the same relay site, they should create two Stations. A Station can carry account balance, login state, groups, multipliers, keys, and collector snapshots.

### Station Key

Routing will eventually happen at the Station Key level, not the Station account level. One Station can have many keys, such as Pro Key, Plus Key, Backup Key, or Test Key.

Phase 3 stores:

- key name
- masked/present API key state for UI
- enabled
- priority
- group / tier
- status
- last checked / used timestamps
- note

## SQLite Tables

### `station_credentials`

Stores login metadata for collection:

- `id`
- `station_id`
- `login_username`
- `login_password`
- `remember_password`
- `login_status`
- `login_error`
- `last_login_at`
- `session_status`
- `session_expires_at`
- `created_at`
- `updated_at`

P3 may store `login_password` as plaintext only as a temporary local prototype. If `remember_password=false`, the password is not persisted. The frontend never displays the full password.

### `station_keys`

Stores multiple API keys under one Station:

- `id`
- `station_id`
- `name`
- `api_key`
- `enabled`
- `priority`
- `group_name`
- `tier_label`
- `status`
- `last_checked_at`
- `last_used_at`
- `note`
- `created_at`
- `updated_at`

P3 may store `api_key` as plaintext only as a temporary local prototype. Commands return `apiKeyMasked` and `apiKeyPresent`, not the full key.

Existing Phase 2 `stations.api_key` values are migrated into one `Default Key` row if a station has no station keys yet. New station creation also creates a `Default Key` from the submitted key.

### `collector_snapshots`

Stores collector runs:

- `id`
- `station_id`
- `source`
- `status`
- `fetched_at`
- `summary_json`
- `normalized_json`
- `raw_json_redacted`
- `error_message`
- `created_at`

Raw data is recursively redacted before persistence. Secret-like fields such as `api_key`, `key`, `token`, `authorization`, `cookie`, `password`, `secret`, and `session` are replaced with `[REDACTED]`.

## Tauri Commands

Station key commands:

- `list_station_keys`
- `create_station_key`
- `update_station_key`
- `delete_station_key`
- `reorder_station_keys`

Credential commands:

- `get_station_credentials`
- `update_station_credentials`
- `clear_station_credentials`

Collector commands:

- `detect_station_info`
- `collect_station_info`
- `detect_sub2api_station`
- `collect_sub2api_station`
- `list_collector_snapshots`
- `get_latest_collector_snapshot`

The generic `detect_station_info` and `collect_station_info` commands are the preferred frontend entry points from P3.1 onward. The old Sub2API-specific commands remain as compatibility wrappers.

## Collector Capability

The P3 collector is a lightweight prototype. It tries short-timeout GET probes:

- `/`
- `/api/pricing`
- `/api/ratio_config`
- `/api/models`
- `/v1/models`

It does not fail the whole run on 404. It records each probe, parses JSON when possible, and recognizes fields such as balance, quota, credit, amount, group, group_name, rate_multiplier, ratio, multiplier, api_key, key, token, usage, used, remain, and remaining.

If credentials exist, the collector records `manual_required` for login status. P3 does not bypass CAPTCHA or 2FA and does not attempt full WebView session capture.

P3.1 changes the execution model:

- Detect has a roughly 5 second global deadline.
- Collect has a roughly 10 second global deadline.
- Single endpoint connect/read timeouts are short, so a bad endpoint cannot hold the whole app.
- Endpoint probes run with controlled concurrency instead of slow serial probing.
- 404 and login-required endpoints are recorded as endpoint results, not fatal task errors.
- Commands return summary / normalized results for the UI, while redacted raw data stays in the snapshot.

## Collector Adapter Direction

Sub2API remains the only real adapter prototype in P3.1. The command layer now exposes generic station information collection entry points and labels the active adapter:

- Sub2API stations use `Sub2API Adapter`.
- NewAPI stations show `NewAPI Adapter（待接入）`.
- OpenAI-compatible stations use a basic adapter label and can identify `/v1/models`.
- Custom stations use `Auto Detect`.

NewAPI support is intentionally not complete in P3.1. It should later prioritize `/api/pricing`, `/api/ratio_config`, and `/api/models`, then normalize groups, multipliers, and model prices into the shared snapshot shape.

## Current Limitations

- No real local proxy.
- No request forwarding.
- No real routing.
- No full WebView login window.
- No XHR/fetch capture.
- No CAPTCHA or 2FA bypass.
- No complete NewAPI adapter.
- No low-token health check.
- No price normalization.

## Security Notes

Phase 3 plaintext password and API key storage is temporary and must not be treated as production secure storage. Before P4 handles real login sessions or proxy traffic, secrets should move to local encryption or the operating system keychain.

The UI warns about this limitation. Logs and snapshots must not include full passwords, cookies, tokens, authorization headers, or API keys.

## P4 Suggestions

- Add WebView login flow.
- Capture authenticated XHR/fetch responses safely.
- Store cookie/session data through encrypted local storage or system keychain.
- Add manual correction for recognized fields.
- Move session and password handling to encrypted local storage or OS keychain before using real authenticated capture.

## P5 Suggestions

- Normalize pricing into CNY per 1M input/output tokens.
- Route by Station Key, not only Station.
- Add low-price routing and fallback policies based on key status, balance, and health.
- Add NewAPI price and ratio collection to the generic collector adapter layer.
