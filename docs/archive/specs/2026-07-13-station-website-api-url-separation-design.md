# Station Website and API URL Separation Design

## Problem

`Station.base_url` currently represents two different resources:

- the website and management origin used for browser opening, login,
  authorization, cookies, remote-key management, and management API collection;
- the OpenAI-compatible upstream base used for model discovery, key probes,
  channel monitors, routing, and proxy forwarding.

The current `collector_base_urls` helper tries to recover both meanings by
adding or removing a trailing `/v1`. That works only when both resources share
an origin and differ by that path segment. Stations whose frontend and API use
different domains cannot be configured correctly. A partial UI-only split
would be worse: downstream consumers could silently continue using the wrong
address.

This design makes both URLs explicit throughout persistence, commands,
frontend state, collectors, authorization, key projections, monitoring, and
routing.

## Goals

- Store an explicit website URL and API base URL for every station.
- Require both values while allowing them to be identical.
- Preserve existing routing behavior during migration as closely as possible.
- Make every network call consume the URL role appropriate to that endpoint.
- Remove ambiguous Station-level `base_url` and `station_base_url` names.
- Validate and normalize URLs in one backend boundary.
- Keep migration atomic, idempotent, and testable against legacy schemas.
- Provide focused extension points for future endpoint roles without building a
  generic endpoint registry prematurely.
- Keep credentials and sensitive URL components out of logs and snapshots.

## Non-Goals

- Adding a "complete URL" mode or storing a final Responses/Chat endpoint.
- Automatically discovering that the website and API use different domains.
- Adding separate model, billing, Anthropic, or provider-specific endpoints.
- Replacing Station columns with a generic endpoint table.
- Changing Station Key ownership, routing policy, fallback, or pricing logic.
- Redesigning provider authentication beyond selecting the correct URL role.

## Considered Approaches

### 1. Add `website_url` while retaining `base_url`

Treat the legacy column as the API URL and add only the website URL. This has a
smaller initial diff, but every maintainer must continue remembering that
`base_url` no longer means a generic base. The ambiguity remains in database
queries, Rust models, and tests.

### 2. Replace the legacy field with two explicit first-class fields

Rename `base_url` to `api_base_url`, add `website_url`, migrate every Station
consumer, and remove Station-level aliases. This has the largest immediate
change surface but produces one stable vocabulary across the application.

This is the selected approach.

### 3. Add a Station endpoint table

Store typed endpoint rows such as website, OpenAI, models, and billing. This is
extensible, but two required values do not justify another table, joins,
ordering rules, and endpoint lifecycle. Explicit columns plus role-specific URL
helpers preserve a clean path to that design if the number of independently
configured endpoint roles later grows.

## Core Invariants

The implementation must maintain these invariants:

1. `website_url` and `api_base_url` are non-empty for every Station row.
2. They may be equal, but neither is derived from the other after migration.
3. Website and management operations never fall back to `api_base_url`.
4. Upstream and routing operations never fall back to `website_url`.
5. A management endpoint uses `website_url` even when it returns models,
   balances, usage, or another routing fact.
6. An OpenAI-compatible endpoint uses `api_base_url` even when called by a
   collector rather than the proxy.
7. Runtime and log types may retain the unambiguous term
   `upstream_base_url`; Station domain types may not retain ambiguous
   `base_url` names.
8. URL validation is authoritative in Rust. Frontend validation is only an
   immediate usability layer.
9. `api_base_url` is the complete API namespace immediately before a resource
   path. It may end in `/v1`, `/api/v3`, or `/api/paas/v4`, but not
   `/responses`, `/chat/completions`, `/models`, or another final resource.
10. A background result may update current Station facts or health only when it
    was produced for the Station's current endpoint revision.

## Data Model and Contracts

### Database

The `stations` table contains:

```sql
website_url TEXT NOT NULL,
api_base_url TEXT NOT NULL,
endpoint_revision INTEGER NOT NULL DEFAULT 1
```

The legacy `base_url` column is removed by migration. The unused
`upstream_api_base_path` column is also removed: it has no runtime consumer and
would create a second, conflicting source of API path semantics. The actively
used `upstream_api_format` column remains unchanged.

Revision-aware derived tables also store the Station revision that produced
their current or historical row:

```sql
collector_snapshots.endpoint_revision INTEGER NOT NULL DEFAULT 1,
collector_runs.endpoint_revision INTEGER NOT NULL DEFAULT 1,
station_endpoint_health.endpoint_revision INTEGER NOT NULL DEFAULT 1,
station_key_health.endpoint_revision INTEGER NOT NULL DEFAULT 1
```

Migration backfills these columns with `1`, matching every migrated Station.
Request logs and channel-monitor runs already preserve the actual upstream URL
used and remain immutable history, so they do not need to masquerade as current
revision state.

### Rust

`Station`, `CreateStationInput`, and `UpdateStationInput` expose:

```rust
pub website_url: String,
pub api_base_url: String,
```

`Station` also exposes `endpoint_revision`. Create initializes it to `1`.
Update increments it only when either normalized URL changes; unrelated Station
edits do not invalidate active endpoint work.

`KeyPoolItem.station_base_url` becomes
`KeyPoolItem.station_api_base_url`. Route candidates continue to expose
`upstream_base_url`, populated only from `stations.api_base_url`. Key Pool
items and route candidates also carry the Station endpoint revision required by
probe, monitor, and runtime-feedback writes.

The login-test contract is also role-specific:

```rust
pub website_url: String,
```

It does not accept an API URL because password login is a management operation.
Its missing-address status changes from `missing_base_url` to
`missing_website_url`, so diagnostics preserve the same vocabulary as the
input contract.

### TypeScript

`Station`, `StationInput`, and `StationUpdateInput` expose `websiteUrl` and
`apiBaseUrl`. `KeyPoolItem.stationBaseUrl` becomes
`stationApiBaseUrl`; Station-derived targets expose `stationEndpointRevision`
where background writes need it. The in-memory Tauri fallback, mocks, fixtures,
query data, and runtime snapshot projection use the same names.

The browser-opening API becomes `openStationWebsite(websiteUrl)`. Generic
external-link helpers remain generic, but Station call sites must use the
role-specific wrapper.

### Historical JSON

Existing collector snapshots and request logs are immutable historical data.
Their embedded legacy `baseUrl` keys are not rewritten. New collector snapshot
payloads use `websiteUrl`, `apiBaseUrl`, or an `endpointRole` plus sanitized
endpoint metadata. Readers must tolerate old snapshots without interpreting
their legacy key as a current Station contract.

## URL Normalization and Validation

One Rust validation function processes both fields before create or update.
It uses a structured URL parser rather than string manipulation.

Accepted URLs:

- use `http` or `https`;
- contain a host, including localhost or an IP address;
- may include a port;
- may include a deployment path such as `/relay`, `/v1`, or `/api/paas/v4`.

Rejected URLs:

- contain embedded username or password data;
- contain a query string or fragment;
- use an unsupported scheme;
- are relative, hostless, or otherwise unparsable;
- use an API Base URL ending in a known final resource path such as
  `/responses`, `/chat/completions`, `/models`, or `/embeddings`.

Normalization trims whitespace and removes redundant trailing slashes while
preserving the origin, port, and meaningful path. Validation errors identify
either the website URL or API Base URL explicitly.

The API Base URL is a complete namespace, not a final request URL. Upstream URL
construction removes the local compatibility prefix (`/v1`) from a validated
incoming operation path and appends only the resource-relative suffix to the
configured namespace:

```text
api_base_url https://relay.example/v1       + /v1/responses
  -> https://relay.example/v1/responses
api_base_url https://ark.example/api/v3     + /v1/chat/completions
  -> https://ark.example/api/v3/chat/completions
api_base_url https://relay.example/proxy/v1 + /v1/models
  -> https://relay.example/proxy/v1/models
```

This is distinct from the rejected "complete URL" mode: the configured value
still stops before `responses`, `chat/completions`, `models`, or another
operation resource, so one Station can serve multiple supported operations.

Frontend forms apply equivalent lightweight checks for field-level feedback,
but a caller cannot bypass the Rust validation by invoking a command directly.

## Database Migration

`migrate_station_endpoint_urls` runs during database initialization before any
Station reader, scheduler, collector, monitor, or proxy candidate query.

For a legacy row:

```text
api_base_url = normalize(ensure_versioned_namespace(old base_url))
website_url  = normalize(remove_terminal_v1(old base_url))
endpoint_revision = 1
```

`ensure_versioned_namespace` preserves a URL whose final path segment is a
version segment such as `/v1`, `/v2`, or `/v4`. It appends `/v1` to an origin
root or non-version terminal path. This preserves the effective standard target
for legacy roots and proxy prefixes while leaving valid provider namespaces
such as `/api/v3`, `/api/paas/v4`, and `/v2` intact. The website transformation
removes only an exact terminal `/v1`, matching the previous common management
derivation without guessing that other provider paths are websites.

The migration cannot infer a truly different legacy website domain. Migrated
stations remain operable under the closest existing semantics and can be
edited afterward. The application must not pretend that it discovered a
different website.

Migration backfill is not treated as a user-initiated origin change. It does
not clear credentials, disable stations, or reset health records merely because
the schema now stores two names for the previously inferred roles.

Migration behavior:

- inspect `PRAGMA table_info(stations)` to recognize legacy, current, and
  partially migrated schemas;
- perform schema changes and backfill in a single SQLite transaction;
- validate every generated pair before commit;
- roll back the complete migration if any row is invalid;
- be safe to run repeatedly;
- produce the new two-column schema for fresh databases;
- remove `upstream_api_base_path` after confirming the current schema contains
  it and no runtime reader remains;
- never modify secret columns, Station IDs, timestamps, priorities, or foreign
  key relationships.

Downgrading to an older application binary after this schema migration is not
supported. Migration failure leaves the legacy schema intact and returns an
actionable startup error.

The schema-state matrix is fail-closed:

- legacy `base_url` only: create and backfill the three new fields, then remove
  the legacy column;
- `api_base_url` present but `website_url` absent: derive only the missing
  website value and initialize the revision;
- both current URL columns present: validate them and add only a missing
  revision;
- legacy and current columns both present with conflicting normalized API
  values: abort with an actionable error instead of choosing silently;
- no usable legacy or current API value: abort without changing the schema.

## Endpoint Ownership

Endpoint ownership follows the remote endpoint being called, not the feature or
collector task name.

### Website and management URL consumers

The following consume `website_url`:

- station-card and pricing-page browser links;
- the Tauri WebView authorization window and initial navigation;
- cookie reads and management-origin session verification;
- password login and login-connection tests;
- Sub2API/NewAPI management endpoints such as identity, groups, rates,
  balances, and remote key CRUD;
- capture-session management requests;
- collector descriptions that identify the managed website.

A model or balance response obtained from a management `/api/...` endpoint
still belongs to this group.

### API and upstream URL consumers

The following consume `api_base_url`:

- OpenAI-compatible `/models` and upstream usage endpoints;
- Station Key model discovery and connectivity probes;
- Station endpoint PING and latency checks;
- channel monitor probes;
- Key Pool projections and search text;
- local routing candidates and runtime snapshots;
- proxy forwarding and fallback attempts;
- request-log `upstream_base_url` values;
- dashboard Key summaries that identify the route target.

PING may normalize a probe path within the configured API origin, but it must
derive that target exclusively from `api_base_url` and never cross to
`website_url`. Key-level authenticated probes remain the source of truth for
whether a particular key can route traffic.

### Authorization request boundary

Captured requests may legitimately target either configured origin, so the
authorization allowlist accepts both `website_url` and `api_base_url`. Matching
uses parsed scheme, host, effective port, and path-segment boundaries. It must
not use a raw prefix comparison that would accept a host such as
`trusted.example.evil.test` or a sibling path with the same textual prefix.

The WebView opens at `website_url`, and cookie lookup remains scoped to the
website origin. Accepting the API origin in the capture allowlist does not cause
API cookies or authorization headers to be persisted automatically; existing
credential extraction and redaction rules still apply.

### Credential-bearing redirects

Management clients must not forward cookies, access tokens, or passwords across
an origin-changing redirect. API clients, connectivity probes, monitors, and
the proxy must not forward Station Key authorization across an origin-changing
redirect. A redirect to a different origin fails with a sanitized endpoint
configuration error unless that flow implements an explicit, separately
reviewed redirect policy. Same-origin redirects may retain existing behavior.

This rule applies even when the redirect target happens to equal the Station's
other configured origin. Endpoint roles do not grant permission to transfer one
role's credentials to the other.

## URL Helper Boundaries

The current derivation-oriented `CollectorBaseUrls` abstraction is removed.
It is replaced by role-specific operations:

- a management URL joiner receives `website_url` and a management path;
- an upstream URL builder receives `api_base_url` plus a validated local
  protocol path, strips exactly one leading compatibility `/v1`, and appends the
  resource-relative remainder to the complete API namespace;
- authorization origin matching receives both configured URLs explicitly;
- no helper returns a website URL from an API URL or vice versa at runtime.

Helpers may normalize path separators. They may not infer a version, silently
change domains, add a provider-specific prefix, accept a final operation URL as
a base, or fall back between roles. All proxy, model-discovery, connectivity,
and channel-monitor call sites share this upstream builder so provider paths do
not diverge by feature.

Local variables and test-server fixtures may still use generic `base_url` when
there is only one unambiguous resource. The naming prohibition applies to
Station fields, Station-derived projections, and public contracts.

## Frontend Experience

### Station create and edit

Both the dedicated Add Provider page and the Station dialog show adjacent
required fields:

- `前端网址`, for example `https://relay.example.com`;
- `API Base URL`, for example `https://api.relay.example.com/v1`.

A small one-shot copy action may copy the website value into the API field for
same-origin stations. It does not create a persistent synchronization mode.
Editing either field later affects only that field.

Login testing validates and sends `websiteUrl`. API health and Key connection
testing validate and send `apiBaseUrl`.

### Provider presets

Every provider preset explicitly declares `websiteUrl` and `apiBaseUrl`.
Implementation must verify the preset values rather than deriving one from the
other. Custom starts with both fields empty. A preset may intentionally use the
same value for both roles.

Preset source-contract tests assert both values, including official providers
whose API path is not `/v1`.

### Station list and details

The Station list uses the website as its clickable primary link and shows the
API Base URL as compact secondary text with an `API` label. Detail views and
property lists show both full values with distinct labels. The Station PING
action states that it checks the API endpoint.

### Key pages

Station endpoints remain owned by Station, not Station Key.

- Add Key displays the selected Station's API Base URL as read-only context.
  The current editable-but-unsaved Base URL field is removed.
- Edit Key displays `stationApiBaseUrl` as read-only context.
- Key Pool search, rows, dashboard Key summaries, and routing views use the API
  value because they describe route targets.

### Other views

- Pricing links open `websiteUrl` because they take the user to the provider.
- Collector page Station descriptions show `websiteUrl`.
- Runtime snapshot and routing diagnostics show `apiBaseUrl`.
- Frontend memory fallbacks and mock Stations always provide both values so a
  browser-only preview cannot hide missing migrations.

## Data Flow

```text
Station form
  -> StationInput { websiteUrl, apiBaseUrl }
  -> Tauri command
  -> Rust parse + normalize + validate both fields
  -> one Station INSERT/UPDATE stores both non-null values
  -> Station readers return both values

websiteUrl branch
  -> browser/login/cookies
  -> management collectors
  -> remote key management

apiBaseUrl branch
  -> Key Pool projection
  -> model/key/channel probes
  -> route candidate
  -> proxy upstream URL
  -> request log
```

Query invalidation remains Station-scoped. Updating either URL invalidates
Station, Key Pool, routing workspace, collector, endpoint-health, and channel
monitor query data because each may embed or depend on one of the values.

## Endpoint Revision and Concurrent Work

Network work captures the Station's `endpoint_revision` when it starts. Any
operation that writes current credentials, facts, freshness, or health passes
that expected revision into the database write. The database compares it with
the current Station revision inside the same transaction as the write.

Revision-aware flows include:

- collector runs, snapshots, normalized fact application, and Station
  collection status;
- capture sessions and native Web authorization completion;
- Station PING and Station Key connectivity results;
- channel monitor health feedback;
- proxy success/failure feedback that updates current Key health.

If the revision no longer matches:

- captured credentials are discarded and never persisted;
- collector output may be retained as a historical snapshot tagged with its
  original revision, but it does not update normalized current facts, freshness,
  Station status, or change-center recovery events;
- the collector run finishes as `superseded`, not `success` or `failed`;
- probe, monitor, and proxy feedback does not repopulate health cleared by the
  endpoint edit;
- request logs may still record a completed in-flight request with the exact
  upstream URL it used, because they are immutable request history.

`collector_runs` records `endpoint_revision` and accepts the terminal
`superseded` status. Station endpoint-health records also store the revision
they describe. Key-health records store the same revision. Key and endpoint
health reads ignore records whose revision does not match the Station.

The UI closes or cancels an active authorization window when an endpoint edit
is accepted. Revision comparison remains the correctness backstop if a window,
collector, or probe completes concurrently after cancellation.

The proxy already reloads route candidates from the database for each request;
this behavior becomes an explicit requirement. A request selected before an
edit may finish against the old upstream and be logged as such. Requests
selected after commit must use the new API URL, or find no candidate when an
API-origin change disabled the Station.

## Endpoint Change Safeguards

An endpoint edit can change where stored secrets are sent. Update logic compares
parsed origins (`scheme`, host, and effective port), not raw URL strings.

When the website origin changes:

- the UI warns that saved login state belongs to the previous website;
- origin-bound cookies, access tokens, session user IDs, and saved login
  passwords are cleared in the same update operation;
- the login username may remain as non-secret convenience data;
- login status becomes reauthorization-required;
- no collector sends old credentials to the new origin automatically.

When only the website path changes within the same origin, session material may
remain, but collector freshness is reset and the next management request must
still handle authentication rejection normally.

When the API origin changes:

- the UI requires explicit confirmation that existing Station Key secrets
  would otherwise be sent to a new host;
- the Station is saved disabled, regardless of its previous enabled state;
- endpoint and Key health become unchecked;
- the user runs an explicit Key connectivity test and then re-enables the
  Station before it can return to routing.

When only the API path changes within the same origin, the Station need not be
disabled, but endpoint and Key health are reset because previous results describe
a different target.

Any change to either endpoint clears Station collection freshness timestamps so
the scheduler treats the Station as due immediately. Collection status returns
to `unchecked` for an enabled Station and remains `disabled` for a disabled
Station. Historical collector snapshots, channel-monitor runs, request logs,
and change events remain immutable history.

The Station update, credential invalidation, routing disablement, freshness
reset, and health reset execute in one database transaction. A failure rolls
back both the new URLs, the revision increment, and their side effects.

## Failure Handling

- If either URL is missing or invalid, create/update fails before writing.
- There is no automatic fallback to the other URL after a network failure.
- Management errors identify the website/management endpoint category.
- Routing and health errors identify the upstream API category.
- Changing `website_url` does not reset Key health because it does not change
  the route target. A website-origin change does clear origin-bound login
  secrets as defined above.
- Changing `api_base_url` invalidates Station endpoint health and prevents stale
  PING results from describing the new target. The update resets the
  `station_endpoint_health` row to `unchecked` in the same database operation.
  Current `station_key_health` rows and cooldowns for the Station's keys are
  cleared, and enabled `station_keys.status` values return to `unchecked`;
  historical request and monitor records remain available.
- Migration errors include the Station ID and field category but do not include
  credentials or response bodies.
- Query strings and embedded credentials are rejected, reducing the chance that
  secrets enter logs, snapshots, browser titles, or error messages through a
  URL.

Provider-page multi-step saving of Station, groups, keys, and credentials keeps
its existing partial-success behavior. This feature guarantees that the two
Station URL columns are written together; it does not redesign the broader
multi-entity save transaction.

## Reliability and Maintainability

- Database constraints enforce presence; Rust validation enforces meaning.
- Migration and fresh-schema paths are covered by the same row readers.
- SQL projections use explicit column lists and names rather than relying on
  legacy positional gaps from `upstream_api_base_path`.
- Role-specific helpers make a wrong URL choice visible at the call site.
- Endpoint revisions prevent late background work from restoring facts,
  credentials, or health for an obsolete configuration.
- Dual-origin integration fixtures detect accidental cross-use better than
  same-origin tests.
- Runtime and logs retain the established, precise term `upstream_base_url`,
  avoiding unrelated churn.
- Historical JSON remains readable without becoming part of the new contract.

## Extensibility

If a future provider needs a separately configured models, billing, Anthropic,
or authorization endpoint, it should first add an explicit optional endpoint
role and a consumer with a clear fallback rule documented at that time. The
website and OpenAI-compatible API fields remain unchanged.

A typed endpoint table becomes justified only when several independent roles
need user configuration, per-role metadata, or repeated endpoints. This design
keeps endpoint selection behind role-specific helpers so that future migration
does not require another ambiguous-field audit.

## Testing Strategy

### Migration tests

- A root legacy URL produces the original website origin and an API namespace
  ending in `/v1`.
- A trailing `/v1` legacy URL preserves the API value and removes only that
  terminal segment for the website.
- `/api/v3`, `/api/paas/v4`, and `/v2` remain complete API namespaces.
- A non-version proxy path gains a terminal `/v1`, while ports, localhost, and
  deployment paths remain structurally valid.
- Invalid legacy data rolls back the complete transaction.
- Re-running the migration is a no-op.
- Fresh and representative older schemas converge on the same current schema.
- Conflicting legacy/current columns fail closed according to the documented
  schema-state matrix.
- IDs, foreign keys, secrets, priorities, and timestamps remain unchanged.
- `upstream_api_base_path` is absent after migration and no query selects it.

### Rust contract and service tests

- Create and update round-trip both URL fields.
- Validation covers schemes, credentials, queries, fragments, ports, and paths.
- Upstream URL construction covers `/v1`, `/v2`, `/api/v3`,
  `/api/paas/v4`, proxy prefixes, and every supported operation path without a
  duplicated compatibility `/v1`.
- Known final resource URLs are rejected as API bases.
- Login and management adapter fixtures receive traffic only on the website
  server.
- OpenAI models, Key probes, channel monitors, and proxy fixtures receive
  traffic only on the API server.
- Remote-key scan/create/update/delete uses the website server.
- PING derives its target only from the API URL.
- Capture matching accepts either configured origin and rejects lookalike hosts
  and sibling path prefixes.
- Credential-bearing clients reject origin-changing redirects without
  forwarding cookies, passwords, access tokens, or Station Keys.
- Route candidates and request logs contain the API value.
- Stale collector, authorization, PING, connectivity, monitor, and proxy
  feedback revisions cannot update current Station state.
- API-origin changes disable routing atomically; same-origin path edits reset
  health without forcing disablement.

### Frontend tests

- Both create/edit entry points require and submit both values.
- Preset selection populates both explicit values.
- Login testing sends only `websiteUrl`.
- Station list/detail labels do not swap the values.
- Pricing links open the website value.
- Add/Edit Key show a read-only API value and never submit a Station URL.
- Key Pool, Dashboard, and runtime snapshot projections use the API value.
- In-memory Tauri fallback create/update behavior round-trips both fields.
- Origin-change warnings describe credential clearing and routing disablement.
- Endpoint edits cancel the active authorization UI and invalidate every query
  family that embeds endpoint-derived state.

### Dual-origin integration fixture

The critical regression test starts two local servers with distinct origins:

```text
management server A: login, identity, groups, rates, remote keys
API server B: models, connectivity probes, monitor requests, proxy requests
```

The test creates one Station using both addresses, exercises collection,
authorization matching, Key probing, monitoring, and routing, then asserts that
each server received only its owned endpoint classes. Same-origin tests remain,
but they cannot prove that the separation is wired correctly.

### Verification commands

- Focused frontend/source contract tests for Station, presets, Key pages, and
  projections.
- Focused Rust database, collector, capture, monitor, and proxy tests.
- Full available TypeScript/Vite checks.
- `cargo fmt --check`, `cargo test`, and `cargo check` for the Tauri crate.

## Documentation Updates

`docs/PROJECT_PLAN.md` and README terminology change from a single Station
Base URL to a website URL plus API Base URL. Collector documentation describes
endpoint role selection rather than the old `/v1` derivation rule. User-facing
copy consistently uses `前端网址` and `API Base URL`.

## Acceptance Criteria

- A Station whose website and API use different origins can log in, collect
  management data, probe keys, run monitors, and route requests successfully.
- No Station domain model, input, projection, or UI form exposes ambiguous
  `baseUrl`, `base_url`, or `stationBaseUrl` fields.
- Management traffic never reaches the configured API server unless that exact
  endpoint is explicitly classified as upstream traffic.
- Proxy, monitor, and Key traffic never reaches the website server.
- Existing root and versioned databases migrate to the same effective standard
  upstream targets under the complete-namespace rule.
- `/api/v3` and other provider namespaces append operation resources without an
  extra `/v1` segment.
- API URL changes invalidate stale endpoint-health presentation.
- Website-origin changes cannot send stored login secrets to the new origin.
- API-origin changes cannot route existing keys until the user tests and
  explicitly re-enables the Station.
- Provider presets and both Station editing surfaces populate both fields.
- The Add Key page no longer presents an editable URL that is silently ignored.
- Authorization origin matching is URL-structure-safe and secret-safe.
- Origin-changing redirects cannot carry Station credentials or Keys.
- Results from an older endpoint revision remain historical and cannot become
  current facts, credentials, or health.
- Historical snapshots and logs remain readable.
- Focused and full available frontend and Rust verification pass.
