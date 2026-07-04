# Channel Monitoring Design

Date: 2026-07-05

## Goal

Upgrade the existing `渠道状态` area into a complete channel health workspace with two in-page tabs:

- `状态`: keep the current real-time channel status view, backed by existing key pool, request log, and station key health data.
- `监控`: add first-class scheduled health monitoring for individual Station Keys or all enabled keys under a Station.

This is a full product capability, not a UI-only patch. It includes persistent monitor configuration, request templates, a background runner, manual execution, execution history, health writeback, and UI management.

## Product Fit

Relay Pool Desktop remains a local desktop tool. The monitoring page should feel like the existing light, compact desktop UI:

- no website landing-page layout;
- no SaaS admin styling;
- no dark theme;
- dense but calm tables, segmented controls, compact badges, and local-tool panels;
- no API key, token, cookie, prompt response body, or raw sensitive header exposure in logs or UI.

The feature must preserve the project model:

- `Station` is the upstream account/site asset.
- `Station Key` is the routable object and health target.
- `station_key_health` remains the health summary consumed by routing and status UI.
- Channel monitoring writes facts into existing health surfaces instead of creating a parallel health universe.

Sub2API's open-source monitor/template structure can be used as product and architecture reference, but Relay Pool Desktop must not copy LGPL implementation code. We reuse concepts only: monitor records, request templates, checker/runner split, execution history, and aggregate availability.

## Page Structure

`渠道状态` stays as the sidebar route. Inside the page header, add a compact `SegmentedControl`:

- `状态`
- `监控`

### Status Tab

The current status tab should remain behaviorally unchanged:

- load `listKeyPoolItems()`;
- load `listRequestLogs()`;
- load `listStationKeyHealth()`;
- combine them into per-key availability, latency, recent outcomes, cooldown, and last error.

Allowed changes:

- fix visible Chinese copy if needed;
- keep current time-window selector;
- refresh when monitor execution updates health;
- optionally add a small hint when a key is monitored.

Do not redesign the status cards as part of this feature unless required for the tab split.

### Monitoring Tab

The monitoring tab contains:

- top toolbar: search, Station filter, target filter, status filter, refresh, `新建监控`, `模板管理`;
- summary strip: enabled monitor count, unhealthy target count, due now count, recent failure count;
- monitor list table or dense list rows;
- optional right-side/detail drawer for selected monitor history.

Monitor list columns:

- name;
- target scope: single key or station all keys;
- station/key summary;
- template;
- primary model;
- interval and jitter;
- last result;
- 7 day availability;
- average latency;
- enabled switch;
- actions: run now, edit, duplicate, delete, history.

Empty state:

- if no keys exist, guide the user to add Station Keys;
- if keys exist but no monitors exist, show `新建监控`;
- do not add tutorial-like explanatory text beyond what is needed.

## Monitor Model

Add `channel_monitors`.

Core fields:

- `id`
- `name`
- `target_type`: `station_key` or `station`
- `station_id`
- `station_key_id`, nullable for station-wide monitors
- `template_id`
- `primary_model`
- `fallback_models_json`
- `enabled`
- `interval_seconds`
- `jitter_seconds`
- `timeout_seconds`
- `max_concurrency`, used for station-wide expansion
- `failure_cooldown_seconds`
- `consecutive_failure_threshold`
- `last_run_at`
- `next_run_at`
- `last_status`: `unchecked`, `success`, `warning`, `failed`, `skipped`
- `last_latency_ms`
- `last_error_summary`
- `created_at`
- `updated_at`

Validation:

- `interval_seconds` range: 15 to 3600.
- `jitter_seconds` range: 0 to 600.
- `interval_seconds - jitter_seconds >= 15`.
- `timeout_seconds` range: 5 to 120.
- station-wide monitors must have `station_id` and no `station_key_id`.
- key monitors must have both `station_id` and `station_key_id`.
- target keys must belong to the selected station.
- template must exist and be enabled.

Station-wide execution expands to all currently enabled Station Keys under the Station. It does not create child monitor records. Each execution writes one run row per key plus a parent summary run.

## Request Template Model

Add `channel_monitor_request_templates`.

Template fields:

- `id`
- `name`
- `provider`: `openai`, `anthropic`, `gemini`, or `custom`
- `protocol`: `openai_chat_completions`, `openai_responses`, `anthropic_messages`, `gemini_generate_content`, or `custom_http`
- `method`
- `path`
- `headers_json`
- `body_template_json`
- `model_field_path`
- `stream_field_path`
- `max_tokens_field_path`
- `default_max_tokens`
- `default_stream`
- `success_rule_json`
- `error_extract_rule_json`
- `description`
- `builtin`
- `enabled`
- `default_for_protocol`
- `version`
- `created_at`
- `updated_at`

Built-in templates:

- OpenAI Compatible default check: `POST /v1/chat/completions`, small `messages`, default `max_tokens`.
- OpenAI Compatible low-token check: `POST /v1/chat/completions`, minimal prompt and very low max token count.
- OpenAI Responses default check: `POST /v1/responses`, minimal `instructions` and `input`.
- OpenAI Responses low-token check: `POST /v1/responses`, minimal payload and low `max_output_tokens`.

The schema should be ready for Anthropic and Gemini templates, but the first implementation may mark them as unsupported until the execution adapter is complete. The UI can still show provider tabs if there are built-in or custom templates for that provider.

Template variables:

- `{{model}}`
- `{{max_tokens}}`
- `{{stream}}`
- `{{challenge}}`
- `{{timestamp}}`

The default challenge should be tiny and deterministic, for example asking for a one-token or short plain response. It must not contain user data.

Template management:

- list templates grouped by provider/protocol;
- create custom template;
- duplicate built-in or custom template;
- edit custom template;
- disable custom template;
- delete custom template only when no monitor references it;
- built-in templates are read-only but can be duplicated.

## Execution History

Add `channel_monitor_runs`.

Fields:

- `id`
- `monitor_id`
- `parent_run_id`, nullable
- `station_id`
- `station_key_id`
- `template_id`
- `target_type`
- `model`
- `status`: `success`, `warning`, `failed`, `skipped`
- `status_code`
- `latency_ms`
- `error_summary`
- `response_excerpt_redacted`
- `checked_at`
- `created_at`

Retention:

- keep enough history for 7 day status and recent UI timelines;
- initial local retention can be count-based, for example newest 2000 rows, plus future daily rollup if needed;
- do not implement cloud sync or notifications in this phase.

Availability calculations:

- 7 day availability for a monitor is successful key-level runs divided by all non-skipped key-level runs in the 7 day window.
- station-wide monitor availability aggregates across expanded key runs.
- current status tab can continue deriving availability from `station_key_health` and request logs.

## Backend Services

Add a monitor service layer rather than placing logic in Tauri commands:

- `ChannelMonitorService`
- `ChannelMonitorRunner`
- `ChannelMonitorTemplateRenderer`
- `ChannelMonitorProbeClient`

Responsibilities:

- service validates CRUD inputs and database writes;
- runner schedules due monitors and manual runs;
- renderer turns template + monitor + key metadata into a safe HTTP request;
- probe client performs the HTTP call with timeout and measures latency;
- result handler records history and updates existing health tables.

Tauri commands:

- `list_channel_monitors`
- `get_channel_monitor`
- `create_channel_monitor`
- `update_channel_monitor`
- `delete_channel_monitor`
- `run_channel_monitor_now`
- `list_channel_monitor_runs`
- `list_channel_monitor_templates`
- `create_channel_monitor_template`
- `update_channel_monitor_template`
- `duplicate_channel_monitor_template`
- `delete_channel_monitor_template`

Startup:

- initialize built-in templates idempotently;
- start the background runner after database and `SecretManager` are ready;
- runner should sleep until the nearest due monitor and re-check after monitor CRUD changes.

Concurrency:

- station-wide monitors expand to enabled keys and run with bounded concurrency, default 3;
- global monitor concurrency should also be bounded to avoid hammering upstreams;
- manual runs can be queued or rejected with a clear `monitor already running` message.

## Health Writeback

Each key-level probe result must update:

- `station_key_health`
- `station_keys.status`
- `station_keys.last_checked_at`

Success:

- increment success count;
- reset consecutive failure count;
- update average latency;
- set last success time;
- clear last error summary;
- clear or ignore cooldown when appropriate.

Failure:

- increment failure count;
- increment consecutive failures;
- set last failure time;
- store a short redacted error summary;
- if consecutive failures reach the threshold, set cooldown using existing routing health semantics.

Skipped:

- do not count as success or failure;
- record why it was skipped, such as disabled key, missing API key, missing template, or unsupported protocol.

Change events:

- optional in the first implementation, but if added, emit only on state transitions or threshold crossings.
- avoid creating an event for every failed interval.

## API And Frontend Types

Add TS API wrappers under `src/lib/api/channelMonitors.ts`.

Add TS types under `src/lib/types/channelMonitors.ts`:

- `ChannelMonitor`
- `CreateChannelMonitorInput`
- `UpdateChannelMonitorInput`
- `ChannelMonitorRun`
- `ChannelMonitorRequestTemplate`
- `CreateChannelMonitorTemplateInput`
- `UpdateChannelMonitorTemplateInput`

Browser fallback:

- keep a memory fallback for frontend preview when Tauri invoke is unavailable;
- fallback can simulate CRUD and manual run state;
- fallback must not imply real scheduled monitoring is active.

## UI Components

`ChannelStatusPage` should become a small page coordinator:

- tab state;
- shared refresh;
- status tab component;
- monitoring tab component.

Suggested files:

- `src/features/channels/ChannelStatusPage.tsx`
- `src/features/channels/ChannelStatusTab.tsx`
- `src/features/channels/ChannelMonitoringTab.tsx`
- `src/features/channels/ChannelMonitorForm.tsx`
- `src/features/channels/ChannelMonitorTemplateManager.tsx`
- `src/features/channels/channelMonitorViewModel.ts`

Form design:

- name;
- target mode segmented control: `单个 Key` / `中转站全部 Key`;
- station selector;
- key selector when target mode is single key;
- template selector;
- primary model input;
- fallback models token input or textarea;
- interval seconds;
- jitter seconds;
- timeout seconds;
- max concurrency for station-wide targets;
- enabled switch;
- advanced section for cooldown and failure threshold.

Template manager design:

- provider tabs;
- compact list rows;
- badges for built-in, default, protocol, linked monitor count;
- actions: duplicate, edit, delete;
- custom editor with method, path, headers JSON, body template JSON, success/error rule JSON.

JSON editor validation can be plain textarea with parse validation in the first implementation. Do not add a heavy editor dependency.

## Security

Never persist or display:

- full API key;
- Authorization header value;
- Cookie or Set-Cookie;
- access token;
- refresh token;
- raw request body if it contains a secret;
- full upstream response body.

Store only:

- status code;
- latency;
- redacted short error summary;
- redacted and truncated response excerpt if useful for diagnosis.

All log and history writes must run through backend redaction, not frontend-only masking.

The runner uses existing `SecretManager` and station key secret resolution. Frontend commands must never receive full key material.

## Error Handling

Monitor CRUD validation errors should be user-readable and specific.

Probe errors should normalize into categories:

- missing key;
- disabled target;
- timeout;
- network error;
- HTTP error;
- protocol mismatch;
- unauthorized;
- rate limited;
- template render error;
- response parse error.

The UI should show a short message in the monitor row and a fuller redacted detail in history.

## Implementation Boundaries

In scope:

- database migrations;
- Rust models, database methods, service, runner, commands;
- TS types and API wrappers;
- monitoring tab UI;
- request template manager;
- status tab split;
- manual and scheduled execution;
- health writeback.

Out of scope:

- desktop notifications;
- email notifications;
- cloud sync;
- replacing the router;
- changing Station/Station Key ownership semantics;
- copying Sub2API code;
- full charting beyond compact availability/timeline indicators.

## Testing And Verification

Rust tests:

- built-in template seeding is idempotent;
- template rendering substitutes model, max tokens, stream, and challenge;
- invalid JSON/template rules are rejected;
- key monitor success updates `station_key_health`;
- key monitor failure increments consecutive failures and can set cooldown;
- station-wide monitor expands enabled keys only;
- skipped runs do not affect health counters;
- redaction removes Authorization, Cookie, token, and key-like fields.

Frontend checks:

- `渠道状态` can switch between `状态` and `监控`;
- existing status view still loads current cards;
- monitor form validates target, interval, jitter, template, and model;
- template manager can duplicate a built-in template and edit custom copies;
- deleting a template referenced by monitors is blocked;
- manual run updates row state and triggers status refresh.

Commands:

```powershell
pnpm.cmd build
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
```

If a command cannot run because of the current dirty worktree or environment, report the exact reason and the best partial evidence.

## Acceptance Criteria

- The `渠道状态` route has `状态` and `监控` tabs.
- `状态` retains the current health cards and refresh behavior.
- Users can create monitors for one Station Key or all enabled keys in one Station.
- Users can choose a request template for each monitor.
- Users can manage built-in-derived and custom templates.
- Background scheduling runs enabled monitors according to interval and jitter.
- Manual `立即检测` works from the UI.
- Probe results write execution history and update existing key health.
- Routing/status consumers continue using `station_key_health`.
- Sensitive values are never exposed in command responses, logs, history, or UI.
- Build/check/test evidence is captured before implementation closeout.
