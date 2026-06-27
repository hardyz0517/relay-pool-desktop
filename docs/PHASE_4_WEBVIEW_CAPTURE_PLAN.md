# Phase 4 WebView Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a fallback WebView login-state capture path and a field normalization rule system so Relay Pool Desktop can collect authenticated station information when the login-state mainline is insufficient.

**Architecture:** Keep the P3/P3.1 station account, station keys, credentials, generic collector commands, and `collector_snapshots` table as the base. Add a login capture window, injected fetch/XHR capture script, Rust capture event bridge, redaction pipeline, extractor rule library, normalized snapshot model, and user confirmation flow. P4 should first prove the WebView capture path, then integrate it into the existing 信息采集 page.

**Tech Stack:** Tauri 2, Wry/WebView, React, TypeScript, Rust, SQLite, existing collector services, existing `collector_snapshots`, local redaction utilities, no large crawler framework.

---

## Background

P2 completed local SQLite persistence for stations/settings. P2.5 reset the UI into a soft relay dashboard with local desktop navigation. P3/P3.1 introduced:

- Station account model.
- Multiple station keys under one station.
- `station_credentials`.
- `station_keys`.
- `collector_snapshots`.
- Generic collector commands: `detect_station_info` and `collect_station_info`.
- Information collection page named `信息采集`.
- Sub2API non-login probe prototype.
- Productized collector UI with raw JSON collapsed into developer details.

P4 changes the collection strategy. The goal is to let the user manually log in to a station backend when needed, capture the page's own fetch/XHR responses as a fallback, redact them, classify them, and normalize them into balance, groups, multipliers, key metadata, models, usage, and endpoint evidence.

## P4 Goals

### User Flow

1. User opens `信息采集`.
2. User selects one station account.
3. User may click `网页登录 / 捕获` from the advanced section only when login-state collection is insufficient.
4. App opens an independent login/capture WebView window for that station's `base_url`.
5. User manually enters account/password and handles CAPTCHA or 2FA if present.
6. App does not bypass CAPTCHA, 2FA, login limits, or site protections.
7. User enters the station backend.
8. App captures fetch/XHR request and response metadata from the backend page.
9. App redacts sensitive values before persistence or frontend display.
10. App identifies balance, groups, multipliers, key metadata, models, usage, and status fields.
11. App saves a redacted collector snapshot.
12. `信息采集` shows a human-readable conclusion.
13. Developer details remain collapsed by default and only show redacted, truncated capture summaries.

### Business Results

P4 should produce:

- Station account login/capture status.
- Latest collector snapshot from WebView capture.
- Captured endpoint list.
- Normalized balance.
- Normalized groups.
- Normalized rate multipliers.
- Normalized model list.
- Detected station key metadata.
- Usage summary when available.
- Confidence score for recognized fields.
- Pending confirmation list for low or medium confidence fields.

## Non-Goals

P4 does not implement:

- Real local proxy.
- Request forwarding.
- Real routing.
- Low-price route selection.
- NewAPI complete adapter.
- HTTPS MITM proxy.
- CAPTCHA bypass.
- 2FA bypass.
- Anti-automation bypass.
- Persistent plaintext cookie/session storage.
- Full price normalization.
- Copying Sub2API source code.
- Large crawler frameworks.
- Complex global state management libraries.

## Existing P3 Interfaces To Preserve

Do not break:

```txt
src-tauri/src/models/collector.rs
src-tauri/src/models/credentials.rs
src-tauri/src/models/station_keys.rs
src-tauri/src/models/stations.rs
src-tauri/src/services/collectors/mod.rs
src-tauri/src/services/collectors/sub2api.rs
src-tauri/src/commands/mod.rs
src/features/collectors/CollectorsPage.tsx
src/lib/api/collector.ts
src/lib/types/collector.ts
src/lib/types/stationKeys.ts
```

P3 generic commands remain available:

```txt
detect_station_info
collect_station_info
list_collector_snapshots
get_latest_collector_snapshot
```

Old Sub2API compatibility wrappers may stay:

```txt
detect_sub2api_station
collect_sub2api_station
```

## Overall Architecture

```txt
信息采集 page
  -> open capture session command
  -> independent station WebView window
  -> injected fetch/XHR capture script
  -> event bridge into Rust
  -> redaction
  -> event classifier
  -> field extractor rules
  -> normalized snapshot builder
  -> collector_snapshots
  -> 信息采集 summary / pending confirmations / developer details
```

Core modules to plan for:

```txt
src-tauri/src/services/capture/
  mod.rs
  session.rs
  redaction.rs
  classifier.rs
  normalizer.rs
  extractors.rs

src-tauri/src/models/capture.rs
src-tauri/src/models/collector.rs
src-tauri/src/commands/mod.rs

src/lib/api/collector.ts
src/lib/types/collector.ts
src/features/collectors/CollectorsPage.tsx
```

Exact file names may be adjusted during implementation to match the current codebase, but keep responsibilities small and separated.

## Technical Route

### Preferred Route: Injected JS Hook

Use a login/capture WebView window and inject a script that wraps:

```txt
window.fetch
XMLHttpRequest.prototype.open
XMLHttpRequest.prototype.send
```

Capture:

- Page URL.
- Request URL.
- Request path.
- Method.
- Status.
- Content type.
- Safe response headers.
- JSON or text response body.
- Started/finished timestamps.
- Duration.
- Error state.

Send captured events to Rust through a Tauri-safe bridge. Redact before saving or displaying.

Reasons:

- Lighter than local proxy.
- More likely to access response bodies than generic WebView request interception.
- Works well for SPA backend requests.
- Keeps capture limited to the station login window.
- Fits user-driven login and CAPTCHA/2FA handling.

Limitations:

- Captures only page JavaScript fetch/XHR.
- May miss iframe, service worker, WebSocket, downloads, and non-JS requests.
- CSP or page isolation may interfere.
- Injection timing must be validated before relying on it.

### Research Route: Native WebView Interception

Research whether Tauri/Wry/WebView2/WKWebView can intercept response bodies. Windows is the first priority because current usage is Windows desktop.

Do not assume cross-platform response body access. If response bodies are unavailable or platform-specific, document this and keep native interception as a supporting path only.

### Fallback Route: Local Proxy Capture

Local proxy capture is not the P4 mainline. HTTPS MITM requires certificate installation and has serious security and trust implications. P4 may document it as a future controlled diagnostic option, but should not implement MITM.

## Capture Event Model

Define a Rust/TypeScript-compatible structure:

```txt
CapturedHttpEvent
  id
  station_id
  source_window_id
  page_url
  request_url
  request_path
  method
  status
  content_type
  started_at
  finished_at
  duration_ms
  response_kind
  response_size
  response_json_redacted
  response_text_preview_redacted
  classification
  confidence
  error_message
```

Rules:

- Redact response body before persistence.
- Truncate large responses.
- Do not save binary response bodies.
- HTML responses should store only a short redacted preview.
- JSON responses may store redacted JSON up to a size budget.
- Cookie, authorization, token, password, secret, session, and API key values must be redacted.
- Events can live in an in-memory queue during capture.
- Not every event must become a database row.
- A session should produce one final `collector_snapshots` row.

## Field Extractor Rule Library

Avoid scattering `if field name contains X` logic across collector code. Introduce rules:

```txt
FieldExtractor
  name
  category
  aliases
  path_patterns
  value_validator
  confidence_weight
```

Extractor categories:

- balance.
- quota.
- usage.
- group.
- group_ratio.
- model_ratio.
- completion_ratio.
- api_key.
- key_metadata.
- model.
- channel_status.
- pricing.
- error.

Extraction output:

```txt
ExtractedField
  category
  label
  value
  source_url
  source_path
  json_path
  confidence
  evidence_preview
```

Rules:

- One normalized field may have multiple candidates.
- High confidence candidates may be adopted automatically.
- Medium confidence candidates go to pending confirmation.
- Low confidence candidates stay in developer details.
- Full API keys are never shown as normal fields.
- Key extraction emits masked/present metadata and context only.

## Normalized Snapshot Target

P4 normalized snapshots should move toward:

```txt
NormalizedCollectorSnapshot
  station_id
  adapter
  status
  captured_at
  login_status
  balance
  currency
  groups[]
  model_rates[]
  group_rates[]
  completion_rates[]
  keys[]
  models[]
  usage_summary
  channel_status
  detected_endpoints[]
  pending_confirmations[]
  confidence_summary
```

Suggested sub-shapes:

```txt
balance
  value
  currency
  source
  confidence
```

```txt
groups[]
  id
  name
  ratio
  source
  confidence
```

```txt
model_rates[]
  model
  prompt_ratio
  completion_ratio
  fixed_price
  group
  source
  confidence
```

```txt
keys[]
  name
  masked_key
  group
  status
  quota
  usage
  source
  confidence
```

```txt
detected_endpoints[]
  url
  method
  status
  classification
  confidence
```

```txt
pending_confirmations[]
  field_category
  candidate_value
  reason
  source
  confidence
```

## Data Storage Design

### Initial P4 Storage

Prefer using existing `collector_snapshots` first:

- `source`: `webview-capture`.
- `status`: `success`, `partial`, `manual_required`, `failed`, or `needs_confirmation`.
- `summary_json`: user-facing summary.
- `normalized_json`: normalized fields, endpoint evidence, confidence, pending confirmations.
- `raw_json_redacted`: redacted and truncated capture summary only.
- `error_message`: short readable error.

### Optional Later Tables

Design these, but add them only when a subphase explicitly needs them.

```txt
captured_events
  id
  station_id
  snapshot_id
  request_url
  method
  status
  content_type
  duration_ms
  classification
  confidence
  response_preview_redacted
  created_at
```

Purpose: bounded diagnostic history. Do not store full sensitive responses.

```txt
field_mappings
  id
  station_id
  adapter
  field_category
  json_path
  source_url_pattern
  confidence
  created_at
  updated_at
```

Purpose: after user confirms a modified-site field once, reuse that mapping later.

```txt
collector_sessions
  id
  station_id
  status
  started_at
  ended_at
  capture_count
  last_error
```

Purpose: track capture windows and user-driven capture lifecycle.

Do not persist cookies or sessions in these tables. If persistent session storage becomes necessary, add an encrypted/keychain storage design first.

## UI Design For 信息采集

The page must remain a collection console, not a raw JSON debug panel.

### Login Capture Card

Add a compact card showing:

- Current login/capture status.
- Recent capture time.
- Captured endpoint count.
- Recognized field count.
- Pending confirmation count.
- Buttons:
  - `网页登录 / 捕获`.
  - `完成采集`.
  - `清除登录状态`.

### Capture Result Summary

Show:

- Balance.
- Group count.
- Rate count.
- Key count.
- Model count.
- Endpoint count.
- Confidence summary.

Empty values should display `未识别`, `暂无`, `需要登录`, or `等待 WebView 捕获`, never `null`, `[]`, or `{}`.

### Pending Confirmations

For medium-confidence fields:

- Field category.
- Candidate value.
- Source endpoint.
- Confidence.
- Actions:
  - Confirm.
  - Ignore.
  - Save field mapping.

Low-confidence fields should stay in developer details.

### Developer Details

Default collapsed. Include:

- Captured endpoint list.
- Redacted event preview.
- Redacted snapshot JSON.
- Copy redacted JSON button.

Do not display full cookies, tokens, passwords, authorization headers, API keys, or unbounded response bodies.

## Security Boundaries

These are hard constraints:

1. Do not save plaintext cookies.
2. Do not save authorization headers.
3. Do not save full tokens.
4. Do not save full API keys in snapshots.
5. Do not save plaintext passwords to snapshots.
6. Do not print sensitive information to logs.
7. Do not send sensitive information to the main UI.
8. Raw preview must be redacted.
9. Large responses must be truncated.
10. Capture window must only target station `base_url` values added by the user.
11. Arbitrary webpages must not be able to call dangerous Tauri commands.
12. Capture script must run only in the login capture window, not the main app window.
13. If the station redirects to a third-party login domain, show a UI warning.
14. Session persistence is off by default.
15. If future session persistence is enabled, local encryption or OS keychain support must be implemented first.

## Subphase Plan

### P4-A: Sub2API Source Audit

**Goal:** Produce a standard Sub2API interface portrait and field dictionary.

**Files:**

- Create or update: `docs/research/SUB2API_SOURCE_AUDIT.md`.
- Read: `D:\Dev\Projects\ai-api-gateway-sub2api\backend\...`.
- Read: `D:\Dev\Projects\ai-api-gateway-sub2api\frontend\...`.
- Do not modify Relay Pool Desktop code.

**Data Structures:** None in Relay Pool Desktop.

**UI Changes:** None.

**Verification:**

- `git diff -- docs/research/SUB2API_SOURCE_AUDIT.md`
- Confirm the audit records repository path, commit hash, frontend API files, backend routes, endpoint table, field dictionary, modified-site fingerprints, and collector impact.

**Risks:**

- Source is incomplete because of partial clone.
- Fork differs from common Sub2API deployments.
- Endpoint paths may not match modified sites.

**Mitigation:**

- Record source completeness.
- Mark all conclusions with confidence.
- Treat audit as rule input, not absolute truth.

**Non-Goals:**

- Do not clone a new repository without user confirmation.
- Do not copy source implementation.
- Do not implement collector changes.

**Completion Standard:**

- Login, user, balance, group, rate, key, model, usage, and channel-status interfaces are mapped.
- Field aliases and confidence notes are ready for extractor rules.

### P4-B: WebView Capture Spike

**Goal:** Verify whether injected fetch/XHR hooks can capture response bodies in a Tauri login window.

**Files:**

- Create: `src-tauri/src/services/capture/mod.rs`.
- Create: `src-tauri/src/services/capture/redaction.rs`.
- Create: `src-tauri/src/models/capture.rs`.
- Modify: `src-tauri/src/commands/mod.rs`.
- Modify: Tauri setup file if a command/window registration point is needed.
- Optional test asset: local HTML fixture under a non-production test location.

**Data Structures:**

- `CapturedHttpEvent`.
- Minimal capture session state in memory.

**UI Changes:**

- Prefer no formal UI.
- A temporary development-only button or command is acceptable if it is clearly scoped and removed or hidden before P4-C.

**Verification:**

- Local test HTML page triggers `fetch` JSON response.
- Local test HTML page triggers XHR JSON response.
- Failed request is captured with readable error.
- Large JSON is truncated.
- Sensitive fields are redacted.
- Closing capture window stops capture.

**Risks:**

- Injection cannot run early enough.
- CSP or page isolation blocks hook behavior.
- Bridge from WebView page to Rust is unsafe if exposed broadly.

**Mitigation:**

- Inject only into the capture window.
- Restrict accepted events by `source_window_id` and station base URL.
- Keep commands narrow and validate payload size.

**Non-Goals:**

- No formal station integration.
- No persistent session storage.
- No field confirmation UI.

**Completion Standard:**

- A spike conclusion documents whether fetch/XHR response body capture works and lists observed limits.

### P4-C: Capture Session MVP

**Goal:** Connect the capture window to real station accounts and produce collector snapshots.

**Files:**

- Modify: `src-tauri/src/services/capture/session.rs`.
- Modify: `src-tauri/src/services/collectors/mod.rs`.
- Modify: `src-tauri/src/commands/mod.rs`.
- Modify: `src/features/collectors/CollectorsPage.tsx`.
- Modify: `src/lib/api/collector.ts`.
- Modify: `src/lib/types/collector.ts`.

**Data Structures:**

- `CollectorCaptureSession`.
- Capture summary in `summary_json`.
- Capture normalized data in `normalized_json`.
- Redacted capture preview in `raw_json_redacted`.

**UI Changes:**

- Keep `网页登录 / 捕获` in an advanced section.
- Add `完成采集`.
- Add `清除登录状态`.
- Show capture status, endpoint count, recognized field count, and pending confirmation count.

**Verification:**

- Select a station and open its `base_url` in the capture window.
- Manually log in.
- Browse dashboard/key/pricing pages.
- Captured endpoint count increases.
- Complete capture creates a `collector_snapshots` row.
- Main UI shows a readable summary.
- Developer details are collapsed and redacted.

**Risks:**

- Window lifecycle bugs leave capture running.
- Third-party login redirects complicate origin checks.
- Snapshot may contain too much data.

**Mitigation:**

- End session when window closes.
- Warn on third-party login domain.
- Store only bounded event summaries and redacted data.

**Non-Goals:**

- No cookie persistence.
- No automatic login.
- No direct writes to station keys from low-confidence capture.

**Completion Standard:**

- User can create a WebView capture snapshot for one real station account without the main app freezing or exposing secrets, but this remains a fallback route rather than the normal flow.

### P4-D: Field Extractor Rules

**Goal:** Normalize captured responses using rule-based extractors with confidence scoring.

**Files:**

- Create: `src-tauri/src/services/capture/extractors.rs`.
- Create: `src-tauri/src/services/capture/normalizer.rs`.
- Modify: `src-tauri/src/models/collector.rs`.
- Modify: `src/lib/types/collector.ts`.

**Data Structures:**

- `FieldExtractor`.
- `ExtractedField`.
- `NormalizedCollectorSnapshot`.
- `PendingConfirmation`.

**UI Changes:**

- Show confidence summary.
- Show recognized balance, groups, rates, keys, models, and usage summary.

**Verification:**

- Unit tests for alias matching.
- Unit tests for JSON path extraction.
- Unit tests for key masking.
- Unit tests for high/medium/low confidence routing.
- Manual test with captured Sub2API dashboard responses.

**Risks:**

- False positives from generic names such as `amount` or `key`.
- Modified sites wrap data in `data`, `result`, `payload`, or `list`.
- Full key values might accidentally reach normalized result.

**Mitigation:**

- Combine path pattern, endpoint classification, and field alias.
- Treat generic fields as medium or low confidence unless endpoint context is strong.
- Redact before extraction and mask key-like values again at output.

**Non-Goals:**

- No full price normalization.
- No automatic route decisions.
- No large ML-based classifier.

**Completion Standard:**

- Captured responses can identify balance, groups, rates, models, and key metadata with confidence and pending confirmations.

### P4-E: User Confirmation UI

**Goal:** Let users confirm, ignore, or save mappings for uncertain fields.

**Files:**

- Modify: `src/features/collectors/CollectorsPage.tsx`.
- Modify: `src/lib/api/collector.ts`.
- Modify: `src/lib/types/collector.ts`.
- Optional DB migration if `field_mappings` is approved.
- Modify: `src-tauri/src/commands/mod.rs`.
- Modify: `src-tauri/src/services/database.rs`.

**Data Structures:**

- `field_mappings` if persisted.
- `ConfirmExtractedFieldInput`.
- `IgnoreExtractedFieldInput`.

**UI Changes:**

- Add pending confirmation list.
- Provide actions: confirm, ignore, save mapping.
- Keep full raw JSON collapsed.

**Verification:**

- Medium-confidence candidate appears in pending confirmations.
- Confirmed field updates latest snapshot summary.
- Ignored field disappears from actionable list.
- Saved mapping is reused by the next capture for the same station.

**Risks:**

- Confirmation UI can become too complex.
- User may confirm the wrong field.

**Mitigation:**

- Show source endpoint and evidence preview.
- Make confirmation reversible at the mapping level.
- Keep the first version narrow: balance, group, rate, model, and key metadata.

**Non-Goals:**

- No global rule marketplace.
- No cross-user/cloud mappings.
- No automatic destructive edits to station keys.

**Completion Standard:**

- Users can resolve uncertain fields without reading raw JSON.

### P4-F: Collector Integration

**Goal:** Feed normalized capture results into existing product surfaces without turning P4 into routing or proxy work.

**Files:**

- Modify: `src/features/collectors/CollectorsPage.tsx`.
- Modify: `src/features/stations/**` only if station row needs collected balance display.
- Modify: `src/features/channels/**` only for partial health/capture status display.
- Modify: `src/features/pricing/**` only for draft captured pricing display.
- Modify: `src-tauri/src/services/database.rs` only if snapshot-derived station balance update is explicitly approved.

**Data Structures:**

- Existing `collector_snapshots`.
- Optional station balance update path.

**UI Changes:**

- Latest snapshot displays normalized result.
- Station row may show collected balance time/source.
- Channel status page may show capture freshness.
- Price table may show draft captured model/rate data.

**Verification:**

- P3 non-login `detect_station_info` and `collect_station_info` still work.
- Station keys still work.
- Credentials still work.
- Settings still persist.
- `pnpm build` passes.
- `cargo check --manifest-path .\src-tauri\Cargo.toml` passes.

**Risks:**

- Integration may overreach into routing or pricing.
- Snapshot-derived values may be low confidence.

**Mitigation:**

- Keep values labelled as captured/draft until confirmed.
- Do not use captured prices for routing in P4.

**Non-Goals:**

- No proxy.
- No route selection.
- No low-price strategy.

**Completion Standard:**

- Capture results are visible in the correct pages as summaries, with confidence and source labels.

## Risk List

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Tauri/Wry cannot stably intercept response bodies across platforms | Native interception path may be unusable | Make JS fetch/XHR hook the P4 mainline; document native limitations |
| JS hook is affected by CSP or page isolation | Some sites may not capture | Validate with spike; fall back to direct collector or manual mapping |
| iframe, WebSocket, service worker, downloads are missed | Capture is incomplete | Document unsupported channels; do not claim full traffic capture |
| Modified sites differ heavily | Extractor rules may miss fields | Use field dictionary, endpoint context, and user confirmations |
| Login/session storage is sensitive | Secret leakage risk | Default session persistence off; no plaintext cookie/session storage |
| Large JSON responses freeze UI | Poor UX and memory pressure | Truncate in Rust, store summaries, keep developer details collapsed |
| Redaction misses secrets | Security incident | Centralize recursive redaction and test secret cases |
| Field extractor false positives | Wrong balance/rates | Use confidence, pending confirmations, and endpoint classifications |
| Confirmation UI becomes too complex | Users avoid using it | Start with a small field set and compact summaries |
| HTTPS MITM is tempting but risky | Certificate/security complexity | Keep MITM out of P4 mainline |
| Real sites have anti-automation | Login may fail or flag automation | User manually logs in; no bypass; respect site behavior |
| WebView cookie session and HTTP collector session are hard to sync | Direct HTTP collector cannot reuse login | Treat WebView capture as separate evidence source; do not rely on cookie sync in P4 |

## Verification Plan

### Technical Spike Verification

- Open local test HTML in the capture window.
- Capture a successful `fetch` JSON response.
- Capture a successful XHR JSON response.
- Capture a failed request.
- Capture a large JSON response and verify truncation.
- Verify redaction of:
  - `api_key`
  - `apikey`
  - `key`
  - `token`
  - `access_token`
  - `refresh_token`
  - `authorization`
  - `cookie`
  - `password`
  - `secret`
  - `session`
- Close the capture window and verify capture stops.

### Real Station Non-Sensitive Verification

- Use a test account or small account.
- Manually complete CAPTCHA or 2FA if needed.
- Browse dashboard, key, pricing, usage, and status pages.
- Confirm captured endpoint list is populated.
- Confirm at least some fields are recognized.
- Confirm main UI does not show secrets.
- Confirm developer details are collapsed and redacted.

### Regression Verification

- P3 non-login detect still works.
- P3 non-login collect still works.
- Station keys create/edit/delete/toggle still work.
- Credentials update/clear still work.
- Settings persistence still works.
- Run `pnpm build`.
- Run `cargo check --manifest-path .\src-tauri\Cargo.toml`.

## Phase Completion Standard

P4 is complete when:

- Sub2API source audit has produced endpoint and field evidence.
- WebView login/capture window works for a station account as a fallback.
- User can manually log in and capture fetch/XHR responses when the login-state mainline is insufficient.
- Captured data is redacted before storage and display.
- A WebView capture run can save a `collector_snapshots` row.
- `信息采集` displays business summaries, not raw JSON.
- Field extraction has confidence scoring.
- Medium-confidence fields can be confirmed or ignored.
- No plaintext cookies, tokens, passwords, authorization headers, or full API keys appear in logs, snapshots, or main UI.
- P3 detect/collect, station keys, credentials, and settings remain usable.

## User Decisions Before Implementation

Before starting P4-B or later, confirm:

- JS fetch/XHR hook is the preferred fallback route for capture spike.
- Session persistence stays off by default in P4.
- New schema additions such as `captured_events`, `field_mappings`, and `collector_sessions` are allowed only when their subphase needs them.
- `D:\Dev\Projects\ai-api-gateway-sub2api` is the correct Sub2API source for audit, or the user wants to provide another path.

## P5 Direction

P5 can build on P4 by adding:

- NewAPI adapter with pricing and ratio endpoints.
- Price normalization to CNY per 1M input/output tokens.
- Confirmed field mapping reuse across modified stations.
- Encrypted local session/key storage or OS keychain.
- Routing by station key.
- Low-price routing and fallback strategies.
- Health checks based on captured key/model availability.

## P4 Execution Notes

P4-A source audit was written to `docs/research/SUB2API_SOURCE_AUDIT.md`.

Important confirmed Sub2API interface facts:

- User management APIs are mounted under `/api/v1`.
- Standard responses use a `{ code, message, data }` envelope.
- Login uses `POST /auth/login`; 2FA can return a temporary token.
- Access and refresh tokens are returned as JSON and stored by the frontend in `localStorage`.
- Frontend requests use `Authorization: Bearer <token>`.
- High-value capture endpoints include `/auth/me`, `/user/profile`, `/user/platform-quotas`, `/keys`, `/groups/available`, `/groups/rates`, `/channels/available`, `/usage/dashboard/*`, and `/channel-monitors`.
- `/keys` may contain a `key` field, so Relay Pool Desktop must always treat captured key values as sensitive and expose only masked/present metadata.

P4-B/C initial implementation should keep capture events in memory and write one `collector_snapshots` row on finish. This avoids high-frequency SQLite writes while the user browses the station backend.
