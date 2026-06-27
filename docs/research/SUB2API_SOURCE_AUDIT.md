# Sub2API Source Audit

## 1. Source Record

Repository path: `D:\Dev\Projects\ai-api-gateway-sub2api`

Commit hash: `2d5364c38c28387e8de0b53631384c3bd7a4beb9`

Remote: `https://github.com/hardyz0517/sub2ap_Z.git` (`blob:none`)

Fork or upstream: local fork/reference repository. Module path is `github.com/Wei-Shaw/sub2api`.

Completeness: complete enough for P4-A endpoint and field analysis. Backend, frontend, migrations, generated Ent models, handlers, services, and repositories are present. Two planned schema files are absent in this fork: `backend/ent/schema/channel.go` and `backend/ent/schema/user_group_rate.go`; channel and user-group-rate behavior was instead audited from migrations, generated models, services, and repositories.

Backend framework: Go, Gin, Ent, PostgreSQL migrations, Redis-backed middleware/rate limiting.

Frontend framework: Vue 3, TypeScript, Vite, Pinia, Vue Router, Axios.

License notes: `LICENSE` is LGPL-3.0. Relay Pool Desktop may use interface knowledge, public field behavior, and endpoint fingerprints from this audit, but must not copy Sub2API implementation code or LGPL-incompatible core logic into Relay Pool Desktop.

Files actually read:

- `backend/cmd/server/main.go`
- `backend/internal/server/router.go`
- `backend/internal/server/routes/auth.go`
- `backend/internal/server/routes/user.go`
- `backend/internal/server/routes/admin.go`
- `backend/internal/server/routes/gateway.go`
- `backend/internal/server/middleware/jwt_auth.go`
- `backend/internal/pkg/response/response.go`
- `backend/internal/handler/auth_handler.go`
- `backend/internal/handler/user_handler.go`
- `backend/internal/handler/api_key_handler.go`
- `backend/internal/handler/usage_handler.go`
- `backend/internal/handler/available_channel_handler.go`
- `backend/internal/handler/channel_monitor_user_handler.go`
- `backend/internal/handler/dto/types.go`
- `backend/internal/handler/dto/mappers.go`
- `backend/internal/service/api_key_service.go`
- `backend/internal/service/group_service.go`
- `backend/internal/repository/user_group_rate_repo.go`
- `backend/internal/repository/channel_repo_pricing.go`
- `backend/internal/repository/pricing_service.go`
- `backend/ent/schema/api_key.go`
- `backend/ent/schema/group.go`
- `backend/ent/schema/user.go`
- `backend/ent/schema/usage_log.go`
- `backend/ent/schema/channel_monitor.go`
- `backend/ent/schema/user_platform_quota.go`
- `backend/migrations/045_add_api_key_quota.sql`
- `backend/migrations/047_add_user_group_rate_multipliers.sql`
- `backend/migrations/082_refactor_channel_pricing.sql`
- `backend/migrations/083_channel_model_mapping.sql`
- `backend/migrations/086_channel_platform_pricing.sql`
- `backend/migrations/125_add_channel_monitors.sql`
- `backend/migrations/142_user_platform_quotas.sql`
- `frontend/src/api/client.ts`
- `frontend/src/api/auth.ts`
- `frontend/src/api/user.ts`
- `frontend/src/api/keys.ts`
- `frontend/src/api/groups.ts`
- `frontend/src/api/channels.ts`
- `frontend/src/api/channelMonitor.ts`
- `frontend/src/api/usage.ts`
- `frontend/src/types/index.ts`
- `frontend/src/router/index.ts`
- `frontend/src/stores/auth.ts`
- `frontend/src/views/auth/LoginView.vue`
- `frontend/src/views/user/DashboardView.vue`
- `frontend/src/views/user/KeysView.vue`
- `frontend/src/views/user/UsageView.vue`
- `frontend/src/views/user/AvailableChannelsView.vue`
- `frontend/src/views/user/ChannelStatusView.vue`

## 2. Project Structure

Backend entry point is `backend/cmd/server/main.go`; route setup is centralized in `backend/internal/server/router.go`. API routes mount under `/api/v1`, while relay-compatible model gateways mount separately under `/v1`, `/v1beta`, `/responses`, `/chat/completions`, `/embeddings`, and image endpoints.

User-facing management APIs are authenticated by JWT middleware. The frontend stores `auth_token`, `refresh_token`, `token_expires_at`, and `auth_user` in `localStorage`, then sends `Authorization: Bearer <token>` from the Axios request interceptor. Axios also sends `Accept-Language` and adds `timezone` to GET query params.

The standard JSON envelope is:

```txt
success: { code: 0, message: "success", data: ... }
error:   { code: <http_status>, message: "...", reason?: "...", metadata?: ... }
page:    data = { items, total, page, page_size, pages }
```

Capability map:

| capability | frontend wrapper | frontend view/store | backend route | backend handler | service/repository | schema/migration |
| --- | --- | --- | --- | --- | --- | --- |
| Login | `frontend/src/api/auth.ts` | `views/auth/LoginView.vue`, `stores/auth.ts` | `POST /api/v1/auth/login` | `AuthHandler.Login` | `AuthService.Login`, token pair service | `users`, TOTP fields |
| Current user/profile | `api/auth.ts`, `api/user.ts` | `stores/auth.ts`, `DashboardView.vue`, `ProfileView.vue` | `GET /api/v1/auth/me`, `GET /api/v1/user/profile` | `AuthHandler.GetCurrentUser`, `UserHandler.GetProfile` | `UserService` | `user.go` |
| Platform quota | `api/user.ts` | `DashboardView.vue` | `GET /api/v1/user/platform-quotas` | `UserHandler.GetMyPlatformQuotas` | `UserPlatformQuotaRepository` | `142_user_platform_quotas.sql`, `user_platform_quota.go` |
| API key list/create/update/delete | `api/keys.ts` | `KeysView.vue` | `/api/v1/keys` | `APIKeyHandler` | `APIKeyService`, `APIKeyRepository` | `api_key.go`, `045_add_api_key_quota.sql`, `064_add_api_key_rate_limits.sql` |
| Groups/rates | `api/groups.ts` | `KeysView.vue`, `AvailableChannelsView.vue` | `GET /api/v1/groups/available`, `GET /api/v1/groups/rates` | `APIKeyHandler` | `GroupRepository`, `UserGroupRateRepository` | `group.go`, `047_add_user_group_rate_multipliers.sql` |
| Channels/models/pricing | `api/channels.ts` | `AvailableChannelsView.vue` | `GET /api/v1/channels/available` | `AvailableChannelHandler.List` | `ChannelService`, channel pricing repo | `082`, `083`, `086` migrations |
| Usage/dashboard | `api/usage.ts` | `DashboardView.vue`, `UsageView.vue`, `KeysView.vue` | `/api/v1/usage...` | `UsageHandler` | `UsageService`, `OpsService` | `usage_log.go`, dashboard migrations |
| Channel status | `api/channelMonitor.ts` | `ChannelStatusView.vue` | `GET /api/v1/channel-monitors`, `GET /api/v1/channel-monitors/:id/status` | `ChannelMonitorUserHandler` | `ChannelMonitorService` | `125_add_channel_monitors.sql`, `channel_monitor.go` |

## 3. Auth And Session Flow

| method | path | auth | request | response fields | aliases | frontend caller | normalized target | confidence | notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/settings/public` | none | none | public settings object | settings, public_settings | `getPublicSettings`, login/register/payment pages | auth_config | High | Loaded before login to decide registration, Turnstile, OAuth, payment visibility. |
| POST | `/api/v1/auth/login` | none | `email`, `password`, `turnstile_token?` | either `access_token`, `refresh_token`, `expires_in`, `token_type`, `user`; or `requires_2fa`, `temp_token`, `user_email_masked` | token, user, two_factor_required | `authAPI.login`, `authStore.login`, `LoginView.vue` | login_status, user | High | Token is returned in JSON, then stored in `localStorage`. |
| POST | `/api/v1/auth/login/2fa` | temp 2FA token in body | `temp_token`, `totp_code` | `access_token`, `refresh_token`, `expires_in`, `token_type`, `user` | totp, two_factor | `authAPI.login2FA` | login_status, user | High | 2FA session is server-side temp token; not normal JWT auth. |
| GET | `/api/v1/auth/me` | Bearer token | none | user/profile fields plus `run_mode` | me, current_user | `authAPI.getCurrentUser`, auth store refresh | user, balance | High | Called after auth state refresh; good WebView capture signal for logged-in state. |
| POST | `/api/v1/auth/refresh` | none, refresh token body | `refresh_token` | `access_token`, `refresh_token`, `expires_in`, `token_type` | refresh_token | Axios interceptor, `authAPI.refreshToken` | session | High | Triggered on 401 when local refresh token exists. Sensitive and noisy for collector; record only endpoint existence. |
| POST | `/api/v1/auth/logout` | none | `refresh_token?` | `message` | logout | `authAPI.logout` | session | High | Frontend clears localStorage even if server revoke fails. |
| POST | `/api/v1/auth/revoke-all-sessions` | Bearer token | none | `message` | revoke_sessions | `authAPI.revokeAllSessions` | session | Medium | Not part of normal capture flow. |

Login page route is `/login`. Public settings are loaded before or during the login page. Login failures use the standard envelope, commonly with HTTP 400/401 and `message`, sometimes `reason` and `metadata`. JWT middleware failures include messages such as `Authorization header is required`, `INVALID_AUTH_HEADER`, `TOKEN_EXPIRED`, `INVALID_TOKEN`, `USER_NOT_FOUND`, `USER_INACTIVE`, and `TOKEN_REVOKED`.

Session side effects:

- Access/refresh tokens are JSON response fields, not HTTP-only cookies for the normal email login flow.
- Frontend persists tokens in `localStorage`.
- Authenticated API requests use `Authorization: Bearer`.
- `withCredentials: true` is enabled, but the standard management API path is token-header driven.
- OAuth flows may use cookies/pending sessions, but P4 collector should treat those as sensitive implementation details and avoid storing cookies.

Capture usefulness:

- `POST /auth/login`, `/auth/login/2fa`, `/auth/refresh`, and `/auth/logout` are sensitive. Capture should classify them as auth/session, redact request and response token values, and avoid persisting full bodies.
- `GET /auth/me` is useful for login confirmation and user/balance extraction.
- `GET /settings/public` is public and useful for site fingerprinting, not for account data.

## 4. User / Balance / Quota Endpoints

| method | path | auth | request | response fields | aliases | frontend caller | normalized target | confidence | notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/auth/me` | Bearer | none | `id`, `email`, `username`, `role`, `balance`, `concurrency`, `rpm_limit`, `status`, `allowed_groups`, `balance_notify_*`, `total_recharged`, `run_mode` | user, balance, amount, credit | auth store refresh | user, balance | High | Strong first logged-in request; balance is direct numeric field. |
| GET | `/api/v1/user/profile` | Bearer | none | same user profile core plus identity bindings and profile source metadata | profile, user | `userAPI.getProfile` | user, balance | High | Good fallback when `/auth/me` missing or modified. |
| PUT | `/api/v1/user` | Bearer | `username?`, `avatar_url?`, balance notification fields | updated profile | profile | `userAPI.updateProfile` | user_settings | Medium | Not useful for collector except fingerprinting. |
| GET | `/api/v1/user/platform-quotas` | Bearer | none | `platform_quotas[]` with `platform`, `daily_limit_usd`, `weekly_limit_usd`, `monthly_limit_usd`, `daily_usage_usd`, `weekly_usage_usd`, `monthly_usage_usd`, window timestamps/reset hints | platform_quota, quota, usage, limit | `DashboardView.vue` | quota | High | Dashboard loads this after login. Useful for per-platform quota buckets. |

User balance fields confirmed:

- `balance`
- `total_recharged`
- `balance_notify_enabled`
- `balance_notify_threshold_type`
- `balance_notify_threshold`
- `status`
- `concurrency`
- `rpm_limit`

Quota fields confirmed:

- API key quota: `quota`, `quota_used`, `expires_at`
- API key rate limit windows: `rate_limit_5h`, `rate_limit_1d`, `rate_limit_7d`, `usage_5h`, `usage_1d`, `usage_7d`, `reset_5h_at`, `reset_1d_at`, `reset_7d_at`
- Platform quota: `daily_limit_usd`, `weekly_limit_usd`, `monthly_limit_usd`, `daily_usage_usd`, `weekly_usage_usd`, `monthly_usage_usd`

## 5. API Key Endpoints

| method | path | auth | request | response fields | aliases | frontend caller | normalized target | confidence | notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/keys` | Bearer | query `page`, `page_size`, `search?`, `status?`, `group_id?`, `sort_by?`, `sort_order?` | paginated `items[]` of API keys | keys, api_keys | `KeysView.vue` | key_metadata | High | List response includes `key` in DTO. Treat as sensitive; store/display masked metadata only. |
| GET | `/api/v1/keys/:id` | Bearer | path id | API key DTO | key | `keysAPI.getById` | key_metadata | High | Ownership checked in handler. |
| POST | `/api/v1/keys` | Bearer | `name`, `group_id?`, `custom_key?`, IP lists, `quota?`, `expires_in_days?`, rate limits | created API key DTO | custom_key, sk, token | `keysAPI.create` | key_metadata | High | Request can contain full custom key; capture must redact request body. |
| PUT | `/api/v1/keys/:id` | Bearer | `name?`, `group_id?`, `status?`, IP lists, `quota?`, `expires_at?`, `reset_quota?`, rate limits, `reset_rate_limit_usage?` | updated API key DTO | toggle, update | `keysAPI.update`, `keysAPI.toggleStatus` | key_metadata | High | There is no separate `/toggle`; frontend toggles by PUT status. |
| DELETE | `/api/v1/keys/:id` | Bearer | path id | `message` | delete_key | `keysAPI.delete` | key_metadata | High | Not useful for collector except endpoint fingerprint. |
| POST | `/api/v1/usage/dashboard/api-keys-usage` | Bearer | `api_key_ids[]` | `stats` keyed by API key id: `api_key_id`, `today_actual_cost`, `total_actual_cost` | key_usage | `KeysView.vue` | usage, key_metadata | High | Called after key list to enrich usage columns. |
| GET | `/api/v1/user/api-keys/:id/usage/daily` | Bearer | query `days` | `items[]`, `days`, `start_date`, `end_date` | daily_usage | usage API wrapper | usage | High | Useful for per-key trend if user opens details. |

API key DTO fields:

```txt
id, user_id, key, name, group_id, status, ip_whitelist, ip_blacklist,
last_used_at, quota, quota_used, expires_at, created_at, updated_at,
rate_limit_5h, rate_limit_1d, rate_limit_7d, usage_5h, usage_1d, usage_7d,
window_5h_start, window_1d_start, window_7d_start, reset_5h_at,
reset_1d_at, reset_7d_at, group
```

Sensitive handling:

- `key`, `custom_key`, `api_key`, `apikey`, `token`, `sk`, and Authorization-like strings must be redacted before storage.
- Even if list responses include full values, Relay Pool Desktop collector should emit only `present: true`, `masked_key`, prefix/suffix if already visible in UI, name, id, group, status, quota, usage, expiry, and last used metadata.

## 6. Groups And Rate Endpoints

| method | path | auth | request | response fields | aliases | frontend caller | normalized target | confidence | notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/groups/available` | Bearer | none | array of groups | groups, available_groups | `KeysView.vue`, `groups.ts` | groups | High | Includes standard active groups user may bind plus subscription groups with active subscription. |
| GET | `/api/v1/groups/rates` | Bearer | none | object map `group_id -> rate_multiplier` | user_group_rates, multipliers | `KeysView.vue`, `AvailableChannelsView.vue` | group_rates | High | `null` becomes `{}` in frontend wrapper. |

Group DTO fields:

```txt
id, name, description, platform, rate_multiplier, rpm_limit, is_exclusive,
status, subscription_type, daily_limit_usd, weekly_limit_usd, monthly_limit_usd,
allow_image_generation, image_rate_independent, image_rate_multiplier,
image_price_1k, image_price_2k, image_price_4k, claude_code_only,
fallback_group_id, fallback_group_id_on_invalid_request,
allow_messages_dispatch, require_oauth_only, require_privacy_set,
created_at, updated_at
```

User-specific rates are backed by `user_group_rate_multipliers(user_id, group_id, rate_multiplier)`. They override/default-join with group `rate_multiplier` in the frontend. Collector should treat `GET /groups/rates` as the strongest evidence for user-specific multiplier and `GET /groups/available` as default group metadata.

## 7. Channels / Models / Pricing Endpoints

| method | path | auth | request | response fields | aliases | frontend caller | normalized target | confidence | notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/channels/available` | Bearer | none | array of channel rows: `name`, `description`, `platforms[]` | available_channels, models, pricing | `AvailableChannelsView.vue` | models, pricing, groups | High | Feature gated; returns `[]` when disabled. |

Channel response shape:

```txt
channel:
  name
  description
  platforms[]
    platform
    groups[]
      id
      name
      platform
      subscription_type
      rate_multiplier
      is_exclusive
    supported_models[]
      name
      platform
      pricing?
        billing_mode
        input_price
        output_price
        cache_write_price
        cache_read_price
        image_output_price
        per_request_price
        intervals[]
          min_tokens
          max_tokens
          tier_label
          input_price
          output_price
          cache_write_price
          cache_read_price
          per_request_price
```

Migration evidence:

- `082_refactor_channel_pricing.sql` adds `billing_mode` and `channel_pricing_intervals`.
- `083_channel_model_mapping.sql` adds channel `model_mapping`.
- `086_channel_platform_pricing.sql` adds platform dimension to `channel_model_pricing`.

Collector implications:

- This endpoint is the primary model/pricing source for logged-in WebView capture.
- It is authenticated and feature-gated, so lack of rows can mean feature disabled, no visible groups, or no active channel, not necessarily a broken station.
- Model identity may appear as `name`, not `model_id`. Modified sites may rename to `model`, `model_name`, `supported_models`, or use nested wrapper objects.

## 8. Usage And Dashboard Endpoints

| method | path | auth | request | response fields | aliases | frontend caller | normalized target | confidence | notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/usage` | Bearer | `page`, `page_size`, `api_key_id?`, `model?`, `request_type?`, `stream?`, `billing_type?`, `start_date?`, `end_date?`, `sort_by?`, `sort_order?`, `timezone` | paginated usage logs | usage, logs | `UsageView.vue`, `DashboardView.vue` | usage | High | User scoped; API key ownership checked. |
| GET | `/api/v1/usage/:id` | Bearer | path id | usage log DTO | usage_detail | `usageAPI.getById` | usage | Medium | Must be ordered after fixed `/stats` and `/dashboard/*` routes by router; source registers `/:id` before stats but Gin exact routes normally win. |
| GET | `/api/v1/usage/stats` | Bearer | `period?`, or `start_date`, `end_date`, `api_key_id?`, `timezone` | usage totals | stats | `UsageView.vue` | usage | High | Good summary fallback. |
| GET | `/api/v1/usage/dashboard/stats` | Bearer | none | dashboard totals | dashboard_stats | `DashboardView.vue` | usage | High | First dashboard data after auth refresh. |
| GET | `/api/v1/usage/dashboard/trend` | Bearer | `start_date?`, `end_date?`, `granularity?`, `timezone` | `trend[]`, `start_date`, `end_date`, `granularity` | trend, daily_usage | `DashboardView.vue` | usage | High | Time-series fields are stable. |
| GET | `/api/v1/usage/dashboard/models` | Bearer | `start_date?`, `end_date?`, `timezone` | `models[]`, `start_date`, `end_date` | model_usage | `DashboardView.vue` | usage, models | High | Model totals and token/cost breakdown. |
| POST | `/api/v1/usage/dashboard/api-keys-usage` | Bearer | `api_key_ids[]` | `stats` map | key_usage | `KeysView.vue` | usage, key_metadata | High | Sensitive only by association; does not include full key values. |
| GET | `/api/v1/user/api-keys/:id/usage/daily` | Bearer | `days`, `timezone` | `items[]`, dates | daily_key_usage | usage wrapper | usage | High | Per-key daily trend. |
| GET | `/api/v1/usage/errors` | Bearer | paging/date/model/api_key/status/category filters | paginated redacted error requests | errors, failures | `UsageView.vue` | errors | Medium | Feature-gated by setting; already redacted by service. |
| GET | `/api/v1/usage/errors/:id` | Bearer | path id | redacted error detail | error_detail | `UsageView.vue` | errors | Medium | Feature-gated and potentially verbose; keep collapsed. |

Usage fields confirmed:

```txt
request_id, model, requested_model, upstream_model, channel_id,
group_id, subscription_id, input_tokens, output_tokens,
cache_creation_tokens, cache_read_tokens, cache_creation_5m_tokens,
cache_creation_1h_tokens, total_tokens, input_cost, output_cost,
cache_creation_cost, cache_read_cost, total_cost, actual_cost,
rate_multiplier, account_rate_multiplier, billing_type, billing_mode,
stream, duration_ms, first_token_ms, user_agent, image_count, image_size,
image_input_size, image_output_size, image_output_tokens, image_output_cost,
created_at
```

Dashboard summary fields include `total_api_keys`, `active_api_keys`, total/today requests, input/output/cache/total token counts, `total_cost`, `total_actual_cost`, `today_cost`, `today_actual_cost`, `average_duration_ms`, `rpm`, `tpm`, and optional `by_platform[]`.

## 9. Channel Status Endpoints

| method | path | auth | request | response fields | aliases | frontend caller | normalized target | confidence | notes |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| GET | `/api/v1/channel-monitors` | Bearer | none | `items[]` monitor cards | channel_status, monitors | `ChannelStatusView.vue` | channel_status | High | Returns `items: []` when feature disabled. |
| GET | `/api/v1/channel-monitors/:id/status` | Bearer | path id | monitor detail with models | monitor_status | `ChannelStatusView.vue` detail dialog | channel_status | High | Used for non-7d windows/detail. |

List item fields:

```txt
id, name, provider, group_name, primary_model, primary_status,
primary_latency_ms, primary_ping_latency_ms, availability_7d,
extra_models[], timeline[]
```

Detail fields:

```txt
id, name, provider, group_name, models[]
  model, latest_status, latest_latency_ms, availability_7d,
  availability_15d, availability_30d, avg_latency_7d_ms
```

Migration `125_add_channel_monitors.sql` confirms monitor storage fields: provider, endpoint, encrypted API key, primary/extra models, group name, enabled, interval, last checked time, and history rows with status, latency, ping latency, message, and checked timestamp.

## 10. Frontend Request Chain

| page | initial/user-action requests | WebView capture | direct HTTP collector | normalized target | confidence source |
| --- | --- | --- | --- | --- | --- |
| Login page `/login` | `GET /settings/public`, then `POST /auth/login`; if needed `POST /auth/login/2fa`; OAuth variants may call pending endpoints | sensitive for POST bodies; useful only for login state | public settings direct; login only user-driven | auth_config, login_status | frontend wrapper and handler |
| Dashboard `/dashboard` | auth store refresh usually calls `GET /auth/me`; page calls `GET /usage/dashboard/stats`, `GET /usage/dashboard/trend`, `GET /usage/dashboard/models`, `GET /usage`, `GET /user/platform-quotas` | useful | requires auth | balance, quota, usage, models | frontend caller |
| Profile `/profile` | `GET /user/profile`; update paths use `PUT /user`, password and binding endpoints | profile GET useful; update sensitive/noisy | requires auth | user, balance | route/handler |
| API keys `/keys` | `GET /keys`, `GET /groups/available`, `GET /groups/rates`, `GET /settings/public`, then `POST /usage/dashboard/api-keys-usage`; create/update/delete use key endpoints | useful but key values sensitive | requires auth | key_metadata, groups, group_rates, usage | frontend caller |
| Available channels `/available-channels` | parallel `GET /channels/available` and `GET /groups/rates` | useful | requires auth and feature enabled | models, pricing, groups, group_rates | frontend caller |
| Usage `/usage` | `GET /usage`, `GET /keys`, `GET /usage/stats`, error tab calls `/usage/errors` and `/usage/errors/:id` | useful; errors may be verbose | requires auth | usage, errors | frontend caller |
| Channel status page | `GET /channel-monitors`; detail/window actions call `GET /channel-monitors/:id/status` | useful | requires auth and feature enabled | channel_status | frontend caller |

Classifier notes:

- WebView capture is strongest when the request order matches SPA behavior: `/auth/me` -> dashboard stats/trend/models -> key/group/channel endpoints.
- Modified sites may rename routes but keep frontend load ordering and response shape. Classifier should combine path pattern, response envelope, field names, and caller-like sequence.
- Auth/token endpoints are sensitive/noisy. Their success is useful for login state but bodies should be dropped or heavily redacted.

## 11. Field Dictionary

Balance and quota:

| aliases | normalized category | confidence rule |
| --- | --- | --- |
| `balance`, `credit`, `amount` | balance | High on `/auth/me` or `/user/profile`; medium elsewhere |
| `quota`, `user_quota`, `available_quota`, `platform_quota` | quota | High on key/platform quota endpoints |
| `quota_used`, `used_quota`, `daily_usage_usd`, `weekly_usage_usd`, `monthly_usage_usd` | quota_usage | High when endpoint path includes `quota`, `keys`, or `platform-quotas` |
| `daily_limit_usd`, `weekly_limit_usd`, `monthly_limit_usd` | quota_limit | High for platform quota or group fields |
| `remain`, `remaining`, `remaining_quota` | remaining_quota | Medium unless path is profile/quota |

Usage:

| aliases | normalized category | confidence rule |
| --- | --- | --- |
| `usage`, `used`, `daily_usage` | usage | Medium generic; high under `/usage` |
| `input_tokens`, `prompt_tokens` | input_tokens | High under usage/dashboard |
| `output_tokens`, `completion_tokens` | output_tokens | High under usage/dashboard |
| `total_tokens` | total_tokens | High under usage/dashboard |
| `request_count`, `requests`, `total_requests`, `today_requests` | request_count | High under usage/dashboard |
| `cost`, `total_cost`, `actual_cost`, `today_actual_cost` | cost | High under usage/dashboard |

Groups and rates:

| aliases | normalized category | confidence rule |
| --- | --- | --- |
| `group`, `groups`, `group_id`, `group_name`, `name` | group | High under `/groups/available`, `/keys`, or channel platform groups |
| `rate_multiplier`, `multiplier`, `group_ratio`, `ratio`, `user_group_rate`, `user_group_rates` | group_rate | High under `/groups/rates`; medium under group objects |
| `model_ratio`, `completion_ratio` | model_rate | Medium unless paired with model/pricing endpoint |
| `rpm_limit` | rate_limit | High under group/user/key DTO |

API keys:

| aliases | normalized category | confidence rule |
| --- | --- | --- |
| `key`, `api_key`, `apikey`, `token`, `access_token`, `refresh_token`, `sk`, `custom_key` | secret_key_value | Always sensitive. Redact and emit only masked/present metadata |
| `key_name`, `name`, `key_id`, `id`, `last_used_at`, `expires_at`, `status` | key_metadata | High under `/keys` |
| `rate_limit_5h`, `rate_limit_1d`, `rate_limit_7d`, `usage_5h`, `usage_1d`, `usage_7d` | key_rate_limit | High under `/keys` |

Models and pricing:

| aliases | normalized category | confidence rule |
| --- | --- | --- |
| `model`, `model_name`, `model_id`, `models`, `supported_models`, `name` | model | High under channels/pricing or usage models |
| `platform`, `provider`, `owned_by` | provider | High under channel/monitor/model endpoints |
| `input_price`, `output_price`, `cache_write_price`, `cache_read_price`, `image_output_price`, `per_request_price`, `fixed_price` | pricing | High under `/channels/available`; medium elsewhere |
| `billing_mode`, `intervals`, `min_tokens`, `max_tokens`, `tier_label` | pricing_tier | High under `/channels/available` |

Channel health:

| aliases | normalized category | confidence rule |
| --- | --- | --- |
| `latency`, `latency_ms`, `primary_latency_ms`, `latest_latency_ms` | latency | High under `/channel-monitors` |
| `ping`, `ping_latency_ms`, `primary_ping_latency_ms` | ping | High under `/channel-monitors` |
| `availability`, `availability_7d`, `availability_15d`, `availability_30d`, `success_rate` | availability | High under `/channel-monitors` |
| `status`, `primary_status`, `latest_status`, `timeline`, `last_checked_at`, `checked_at`, `last_error` | channel_status | High under `/channel-monitors` |

Error and auth:

| aliases | normalized category | confidence rule |
| --- | --- | --- |
| `error`, `message`, `code`, `reason`, `metadata` | error | High in standard envelope failure |
| `unauthorized`, `forbidden`, `login_required`, `TOKEN_EXPIRED`, `INVALID_TOKEN`, `TOKEN_REVOKED` | auth_error | High on 401/403 |
| `requires_2fa`, `two_factor_required`, `captcha_required`, `turnstile_token` | auth_challenge | High on auth routes |

## 12. Modified-Site Fingerprints

High confidence fingerprints:

- API base path `/api/v1`.
- Standard envelope `{ code, message, data }` with `code: 0` success.
- Login path sequence: `/settings/public` then `/auth/login`, optional `/auth/login/2fa`, then `/auth/me`.
- Token field names `access_token`, `refresh_token`, `expires_in`, `token_type`.
- User balance field `balance` on current-user/profile payloads.
- Key management paths `/keys`, `/keys/:id` and fields `key`, `name`, `group_id`, `quota`, `quota_used`, `expires_at`, `last_used_at`.
- Group paths `/groups/available`, `/groups/rates` and fields `rate_multiplier`, `platform`, `subscription_type`, `is_exclusive`.
- Channel/pricing fields `supported_models`, `input_price`, `output_price`, `cache_write_price`, `cache_read_price`, `per_request_price`, `intervals`.
- Usage dashboard paths `/usage/dashboard/stats`, `/usage/dashboard/trend`, `/usage/dashboard/models`.
- Channel monitor paths `/channel-monitors`, `/channel-monitors/:id/status`.

Medium confidence fingerprints:

- Frontend SPA route names `/dashboard`, `/keys`, `/usage`, `/available-channels`, `/profile`, `/channel-status` or equivalent menu entries.
- `timezone` query param added to GET requests.
- Public settings loaded by multiple auth/payment/register pages.
- Feature-gated endpoints returning empty arrays or `{ items: [] }` rather than 404.
- User-specific group rates as an object map keyed by group id.

Likely drift:

- Balance renamed to `credit`, `amount`, `remain`, `remaining`, or wrapped under `data.user`.
- Extra wrappers such as `result`, `payload`, `items`, `list`, or nested `data.data`.
- API key list may hide full key values or return only masked values.
- Pricing units may drift from per-token to per-1K/per-1M/per-request display units.
- `/channels/available` may be disabled, renamed, or admin-only in modified deployments.
- OAuth/CAPTCHA/Turnstile fields may vary by deployment.
- Obfuscated frontend bundles can hide source names, but request ordering remains valuable.

## 13. Relay Pool Desktop Collector Impact

Rule conversion table:

| source evidence | endpoint path pattern | response shape | field aliases | normalized category | value validator | sensitive handling | confidence weight | UI destination |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `AuthHandler.GetCurrentUser`, `UserHandler.GetProfile` | `/auth/me|/user/profile` | object user/profile | `balance`, `credit`, `amount`, `remaining` | balance | numeric string or number | not secret | high on auth/profile path | 采集结果摘要 |
| `GetMyPlatformQuotas` | `/user/platform-quotas` | `{ platform_quotas: [] }` | `daily_limit_usd`, `daily_usage_usd`, `monthly_limit_usd`, `monthly_usage_usd` | quota | numeric/null per platform | not secret | high | 配额摘要 |
| `APIKeyHandler.List` | `/keys$|/api-keys` | paginated `{ items: [] }` or array | `id`, `name`, `key`, `group_id`, `quota`, `quota_used`, `expires_at`, `last_used_at`, `status` | key_metadata | string/id/date/numeric | mask full key; store only present/masked metadata | high | API Keys summary |
| `APIKeyHandler.GetAvailableGroups` | `/groups/available` | array | `id`, `name`, `platform`, `rate_multiplier`, `is_exclusive`, `subscription_type` | groups | group id/name plus numeric multiplier | not secret | high | 分组摘要 |
| `APIKeyHandler.GetUserGroupRates` | `/groups/rates` | object map | `rate_multiplier`, numeric values | group_rates | map key numeric-ish, value number | not secret | high | 倍率摘要 |
| `AvailableChannelHandler.List` | `/channels/available` | array channels with platforms | `supported_models`, `pricing`, `input_price`, `output_price`, `intervals` | models/pricing | model string plus nullable numeric prices | not secret | high if path matches; medium by field shape | 模型/价格草稿 |
| `UsageHandler.DashboardStats` | `/usage/dashboard/stats` | object totals | `total_requests`, `total_tokens`, `total_actual_cost`, `today_*`, `rpm`, `tpm` | usage | non-negative numeric | not secret | high | 用量摘要 |
| `UsageHandler.DashboardTrend` | `/usage/dashboard/trend` | `{ trend: [] }` | `date`, `requests`, token/cost fields | usage_trend | date and numeric | not secret | high | 趋势详情 |
| `UsageHandler.DashboardModels` | `/usage/dashboard/models` | `{ models: [] }` | `model`, token/cost fields | model_usage | model string plus numeric | not secret | high | 模型用量 |
| `ChannelMonitorUserHandler.List` | `/channel-monitors$` | `{ items: [] }` | `primary_status`, `latency_ms`, `ping_latency_ms`, `availability_7d`, `timeline` | channel_status | status enum/string, numeric latency | not secret | high | 渠道状态 |
| standard response package | any `/api/v1/*` | `{ code, message, data }` | `code`, `message`, `reason`, `metadata` | endpoint_fingerprint/error | code number/string | redact metadata if secret-like | medium | Developer details |

Capture policy:

- Capture after user-driven login only. Do not bypass CAPTCHA, 2FA, rate limits, or anti-automation.
- Drop or aggressively redact bodies for `/auth/login`, `/auth/login/2fa`, `/auth/refresh`, `/auth/logout`, OAuth pending endpoints, and any request containing password/token/cookie/authorization fields.
- For key endpoints, classify as useful but sensitive. Redaction must run before persistence and again before normalized output.
- Prefer frontend request sequence plus response fields over path alone for modified sites.
- Persist redacted bounded summaries in `collector_snapshots`: endpoint list, categories, confidence, and normalized fields.
- Low/medium confidence generic fields such as `amount`, `key`, or `status` should go to pending confirmation or developer details unless endpoint context is strong.
- A missing `/channels/available` result should not mark station unsupported; it may be feature disabled.

Suggested normalized snapshot categories from this audit:

```txt
login_status, balance, quota, groups, group_rates, key_metadata,
models, pricing, usage_summary, usage_trend, model_usage,
channel_status, auth_errors, endpoint_fingerprints
```

## 14. Open Questions

- This fork includes a full user-facing channel/pricing endpoint, but `backend/ent/schema/channel.go` is absent. The exact schema source for channels may be generated from another package or removed from this fork; migrations and repository code are still enough for field extraction.
- `backend/ent/schema/user_group_rate.go` is absent. The source of truth for user-specific group rates is the SQL migration and repository/service path.
- The API key DTO returns a `key` field. It is not confirmed from runtime whether production deployments always return full key values, masked values, or hide them after creation. Collector must assume it can be full secret and mask it.
- `/channels/available` and `/channel-monitors` are feature-gated. A modified site may have these disabled, renamed, or admin-only.
- Pricing unit semantics are not fully normalized by this audit. Fields are numeric but may represent per-token, per-request, or display units depending on `billing_mode` and deployment.
- OAuth flows and pending-auth cookies were only partially audited because P4 collector should not persist or depend on OAuth internals.
- The reference is a local fork, not proven to represent all Sub2API deployments. Treat all rules as weighted evidence, not absolute protocol law.
