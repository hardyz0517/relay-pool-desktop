# Sub2API Source Audit Plan

## Audit Goal

P4 must stop blindly guessing endpoints and instead build a reliable picture of standard Sub2API behavior. This audit is a planning and research task only. It must identify login flow, user/account APIs, key management APIs, pricing/ratio/model APIs, frontend request chains, response field shapes, and modified-site field fingerprints. Do not copy Sub2API implementation code into Relay Pool Desktop.

The audit output feeds Relay Pool Desktop's collector rules, WebView capture classifier, normalized snapshot model, and user confirmation UI.

## Local Source Location Check

Local source was found:

```txt
Path: D:\Dev\Projects\ai-api-gateway-sub2api
HEAD: 2d5364c38c28387e8de0b53631384c3bd7a4beb9
Remote: https://github.com/hardyz0517/sub2ap_Z.git
Remote mode observed: blob:none
Likely status: local fork/reference repository
```

Also observed nearby:

```txt
D:\Dev\Projects\ai-api-gateway
```

The P4-A audit should record whether `ai-api-gateway-sub2api` is complete enough for backend and frontend endpoint analysis. If important files are missing because of partial clone or sparse checkout, stop the audit and ask the user whether to hydrate the repository or provide another source path.

Missing common paths checked during planning:

```txt
D:\Dev\Projects\sub2api
D:\Dev\Projects\Sub2API
```

## Source Files To Read

### Backend Structure

Start with these files and directories:

```txt
D:\Dev\Projects\ai-api-gateway-sub2api\backend\cmd\server\main.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\server\router.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\server\routes\auth.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\server\routes\user.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\server\routes\admin.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\server\routes\gateway.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\handler\auth_handler.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\handler\user_handler.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\handler\api_key_handler.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\handler\usage_handler.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\handler\available_channel_handler.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\handler\channel_monitor_user_handler.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\service\api_key_service.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\service\group_service.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\service\user_group_rate.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\service\channel_service.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\repository\pricing_service.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\internal\repository\channel_repo_pricing.go
```

### Backend Schema And Migration Clues

Read these schema files and related migrations:

```txt
D:\Dev\Projects\ai-api-gateway-sub2api\backend\ent\schema\api_key.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\ent\schema\group.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\ent\schema\user.go
D:\Dev\Projects\ai-api-gateway-sub2api\backend\ent\schema\usage_log.go
```

Important migration names to inspect:

```txt
045_add_api_key_quota.sql
047_add_user_group_rate_multipliers.sql
082_refactor_channel_pricing.sql
083_channel_model_mapping.sql
086_channel_platform_pricing.sql
125_add_channel_monitors.sql
142_user_platform_quotas.sql
```

### Frontend API And Request Chain

Read these frontend API wrappers first:

```txt
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\api\auth.ts
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\api\user.ts
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\api\keys.ts
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\api\groups.ts
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\api\channels.ts
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\api\channelMonitor.ts
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\api\usage.ts
```

Then trace call sites through:

```txt
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\router
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\views
D:\Dev\Projects\ai-api-gateway-sub2api\frontend\src\components
```

## A1. Source Record Output

The audit document should start with:

```txt
Repository path:
Commit hash:
Remote:
Fork or upstream:
Completeness:
Backend framework:
Frontend framework:
License notes:
Files actually read:
```

License notes must mention that Relay Pool Desktop may use interface knowledge and field behavior, but must not copy AGPL/LGPL-incompatible core implementation.

## A2. Project Structure Audit

Record:

- Backend entry point.
- Router initialization.
- Route groups and middleware.
- Auth middleware and session/token handling.
- User/profile handlers.
- API key handlers.
- Group/rate handlers.
- Channel/model/pricing handlers.
- Usage/dashboard handlers.
- Channel monitor handlers.
- Frontend route layout.
- Frontend API wrapper conventions.
- Frontend request client, interceptors, token refresh, and error handling.

The structure section should produce a map like:

```txt
Capability: API key list
Frontend wrapper: frontend/src/api/keys.ts
Frontend view:
Backend route:
Backend handler:
Service/repository:
Schema/migration:
```

## A3. Login Flow Audit

Observed candidate endpoints from initial planning:

```txt
POST /auth/login
POST /auth/login/2fa
GET  /auth/me
POST /auth/logout
POST /auth/refresh
GET  /settings/public
```

The audit must confirm exact methods and paths from source, then record:

- Login page route.
- Login API path and method.
- Login request body fields.
- Login response fields.
- Whether tokens are returned in JSON, headers, cookies, or local storage.
- Cookie/session attributes if present.
- Token refresh behavior.
- Logout API behavior.
- Login failure response shape.
- 2FA request and response shape.
- CAPTCHA or email verification flow if present.
- Public settings needed before login.
- Which user info endpoints are called immediately after login.
- Which frontend state fields indicate logged-in state.

Use this endpoint template:

```txt
Endpoint:
Method:
Path:
Auth required:
Request body:
Query params:
Response JSON sample:
Success fields:
Failure fields:
Session/token side effects:
Frontend callers:
Capture usefulness:
Direct HTTP collector usefulness:
Notes:
```

## A4. Information Interface Audit

Audit these capability groups.

### User, Balance, Quota

Observed candidate endpoints:

```txt
GET /user/profile
GET /user
GET /user/platform-quotas
```

Record fields for:

- User id/name/email.
- Balance.
- Quota.
- Remaining quota.
- Used quota.
- Credit or amount.
- Currency or platform unit.
- Platform-specific quota buckets.
- Account status.

### API Keys

Observed candidate endpoints:

```txt
GET    /keys
POST   /keys
GET    /keys/{id}
PATCH  /keys/{id}
DELETE /keys/{id}
POST   /keys/{id}/toggle
```

Confirm exact paths and methods. Record:

- Key id.
- Key name.
- Masked or full key handling.
- Custom key field.
- Group id/name.
- Quota.
- Usage.
- Rate limits.
- Enabled/disabled state.
- Expiration fields.
- Last used/check fields if present.
- Whether key list responses include full key values.

Relay Pool Desktop must only store/display masked key metadata by default.

### Groups And Rates

Observed candidate endpoints:

```txt
GET /groups/available
GET /groups/rates
```

Record:

- Group id.
- Group name.
- Group ratio.
- User-specific group multiplier.
- Available group constraints.
- Default group behavior.
- Fallback/default rates.

### Channels, Models, Pricing

Observed candidate endpoints:

```txt
GET /channels/available
```

Observed pricing-like fields to verify:

```txt
input_price
output_price
cache_write_price
cache_read_price
image_output_price
per_request_price
intervals
supported_models
```

Record:

- Channel/provider grouping.
- Model ids.
- Model display names.
- Platform/provider fields.
- Group-specific availability.
- Input/output price units.
- Cache price units.
- Image or per-request price units.
- Completion ratio or model ratio fields.
- Whether endpoint is public or logged-in.

### Usage And Dashboard

Observed candidate endpoints:

```txt
GET /usage
GET /usage/stats
GET /usage/dashboard/stats
GET /usage/dashboard/trend
GET /usage/dashboard/models
GET /user/api-keys/{id}/usage/daily
GET /usage/dashboard/api-keys-usage
GET /usage/errors
```

Record:

- Usage summary.
- Daily usage.
- Model usage.
- Key usage.
- Token fields.
- Request count.
- Cost/quota fields.
- Error fields.
- Time range parameters.

### Channel Status

Observed candidate endpoints:

```txt
GET /channel-monitors
GET /channel-monitors/{id}/status
```

Record:

- Latency.
- Ping.
- Availability.
- Recent status timeline.
- Last checked time.
- Error summary.
- Whether this maps to Relay Pool Desktop's 渠道状态 page.

## A5. Frontend Request Chain Audit

For each important page, record what it calls on initial load and user actions:

- Login page.
- Dashboard/home page.
- User profile/settings page.
- API key management page.
- Groups page.
- Pricing/channel/model page.
- Usage page.
- Channel status page.

For each request, classify:

```txt
WebView capture: useful / noisy / sensitive / skip
Direct HTTP collector: useful / requires auth / public / not useful
Normalized target: balance / groups / rates / keys / models / usage / channel_status
Confidence source: path / field names / response shape / frontend caller
```

Important P4 principle: the frontend request chain is often more useful than backend route names because modified sites may keep the same SPA request order even when route names or fields drift.

## A6. Field Dictionary

The audit should produce a field dictionary that can be converted into extractor rules.

### Balance And Quota

```txt
balance
quota
credit
amount
remain
remaining
user_quota
platform_quota
available_quota
used_quota
```

### Usage

```txt
usage
used
used_quota
prompt_tokens
completion_tokens
total_tokens
request_count
daily_usage
model_usage
key_usage
```

### Groups

```txt
group
group_id
group_name
group_ratio
usable_group
available_group
user_group
```

### Ratios And Multipliers

```txt
ratio
rate_multiplier
multiplier
model_ratio
group_ratio
completion_ratio
user_group_rate
```

### API Keys

```txt
key
api_key
apikey
token
access_token
sk
custom_key
key_name
key_id
```

Key values must be treated as sensitive. Extractors should emit masked/present metadata, not full key values.

### Models And Pricing

```txt
model
model_name
model_id
models
owned_by
supported_models
input_price
output_price
cache_write_price
cache_read_price
image_output_price
per_request_price
fixed_price
intervals
```

### Channel Health

```txt
latency
ping
availability
success_rate
status
timeline
last_checked_at
last_error
```

### Error And Auth

```txt
error
message
code
unauthorized
forbidden
login_required
two_factor_required
captcha_required
```

## A7. Modified-Site Adaptation Inference

The audit should infer which parts modified Sub2API sites are likely to keep:

- Frontend route names and bundled API paths.
- Auth endpoints.
- User/profile endpoint shapes.
- API key list response fields.
- Group/rate field names.
- Model/pricing endpoint names.
- Dashboard request order.
- Channel monitor response shape.

Also record likely drift:

- Renamed balance/quota fields.
- Custom pricing units.
- Removed public pricing endpoints.
- Login-required ratio endpoints.
- Obfuscated frontend bundles.
- API key values hidden from the UI.
- Extra wrapper objects such as `data`, `result`, `payload`, `items`, or `list`.

For each inference, mark confidence:

```txt
High: confirmed by standard source and common response shape.
Medium: inferred from frontend usage or schema naming.
Low: guessed from naming only; must go to developer details or user confirmation.
```

## Output Format

The final audit document should include these sections:

```txt
1. Source Record
2. Project Structure
3. Auth And Session Flow
4. User / Balance / Quota Endpoints
5. API Key Endpoints
6. Groups And Rate Endpoints
7. Channels / Models / Pricing Endpoints
8. Usage And Dashboard Endpoints
9. Channel Status Endpoints
10. Frontend Request Chain
11. Field Dictionary
12. Modified-Site Fingerprints
13. Relay Pool Desktop Collector Impact
14. Open Questions
```

For each endpoint, use this normalized table:

```txt
method | path | auth | request | response fields | aliases | frontend caller | normalized target | confidence | notes
```

## Turning Audit Into Collector Rules

After audit, create a rule conversion table:

```txt
Source evidence:
Endpoint path pattern:
Response shape:
Field aliases:
Normalized category:
Value validator:
Sensitive handling:
Confidence weight:
UI destination:
```

Examples:

```txt
Endpoint path pattern: /user/profile|/user|/auth/me
Field aliases: balance, quota, credit, amount, remain, remaining, user_quota
Normalized category: balance
Value validator: numeric string or number, non-negative when possible
Sensitive handling: not secret
Confidence weight: high when endpoint is authenticated user/profile, medium elsewhere
UI destination: 采集结果摘要
```

```txt
Endpoint path pattern: /keys
Field aliases: key, api_key, custom_key, token, sk
Normalized category: key_metadata
Value validator: string with key-like shape
Sensitive handling: mask value, store only present/masked metadata in normalized result
Confidence weight: high for key list endpoint, low elsewhere
UI destination: API Keys summary and pending confirmations
```

Do not introduce rules that require bypassing CAPTCHA, 2FA, anti-automation, or unauthorized access. P4 relies on user-driven login and capture of responses visible to that logged-in session.
