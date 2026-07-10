# Relay Pool Billing And Pricing Architecture Design

Date: 2026-07-10

## Purpose

Relay Pool Desktop needs one durable billing and pricing architecture for local proxy requests, channel monitoring, request logs, dashboard totals, and pricing diagnostics.

The current implementation mixes several responsibilities:

- model base prices
- station group multipliers
- manual pricing overrides
- request-time cost calculation
- read-side request log backfill
- dashboard aggregation
- pricing page projections

That creates split behavior: one surface can know a base price or multiplier while another still reports the request as unpriced. The upgrade should replace that with a single backend-owned pricing context and a single cost calculator.

This is a large refactor. The goal is long-term maintainability, not a narrow display patch.

## Non-Goals

- Do not add accounts, payments, subscriptions, cloud sync, or team permissions.
- Do not build a full financial ledger for a local desktop tool in the first stage.
- Do not copy a third-party gateway's product shape, naming, schema, comments, or UI.
- Do not put specific third-party project names into new billing architecture code, tests, schema, comments, or UI copy. Use neutral domain names.
- Do not make React pages or TypeScript projections the source of billing truth.

## Target Principle

Pricing truth is resolved once in Rust, then consumed everywhere.

The frontend may project and explain data, but it must not decide the real request cost. Proxy runtime, channel monitoring, request log writes, request log reads, and dashboard totals must all agree because they share the same resolved context and calculator.

## Domain Boundaries

### Price Facts

Price facts store observed or configured pricing data. They do not calculate a request cost.

Examples:

- model base prices
- station group rate facts
- manual pricing overrides
- station key group bindings

### Pricing Resolver

The pricing resolver answers:

> Given this station key and requested model, what pricing context applies right now?

It returns a `ResolvedPricingContext`. This is the only public backend result that downstream runtime code should use for request pricing.

### Cost Calculator

The cost calculator answers:

> Given a resolved pricing context and request usage, what is the cost breakdown?

It owns formulas for token cost, fixed cost, future cache-token cost, future media cost, and currency handling.

### Request Cost Snapshot

Request logs store the pricing context and computed cost from request time. Historical request cost must not silently change when current model prices or group multipliers change later.

### Display Aggregation

Dashboard, logs, and pricing pages display resolved facts and snapshots. They do not recalculate authoritative request cost from low-level tables.

## Core Data Types

### `ResolvedPricingContext`

This should be the central backend DTO used by proxy runtime, channel monitoring, request log writes, and diagnostic commands.

Suggested fields:

- `station_key_id`
- `station_id`
- `requested_model`
- `resolved_model`
- `request_kind`
- `group_binding_id`
- `base_input_price`
- `base_output_price`
- `base_fixed_price`
- `currency`
- `unit`
- `base_price_source`
- `effective_rate_multiplier`
- `rate_source`
- `rate_collected_at`
- `estimated_input_price`
- `estimated_output_price`
- `estimated_fixed_price`
- `pricing_status`
- `confidence`
- `source_chain`
- `reason`
- `resolved_at`

The context should be serializable so it can be saved into request logs as an audit snapshot.

### `PricingStatus`

Use explicit statuses instead of a single vague unpriced state:

- `priced`: complete pricing context exists.
- `base_price_only`: model base price exists, but no trusted multiplier was found; use `1.0x`.
- `missing_rate`: model base price exists, and the key or group expected a multiplier, but no current multiplier fact was available.
- `missing_model_price`: multiplier or binding exists, but the requested model has no base price and no matching manual override.
- `unpriced`: no usable model price and no usable override.
- `unsupported_billing_mode`: data exists, but the calculator does not support that mode yet.
- `legacy_estimate`: old request log rows that lack request-time pricing snapshots.

Only `unpriced` should display as true "未定价".

### `RequestUsage`

Initial shape:

- `input_tokens`
- `output_tokens`
- `total_tokens`
- `request_count`

Future-safe fields can be added when needed:

- `cache_creation_tokens`
- `cache_read_tokens`
- `media_count`
- `duration_seconds`
- `size_tier`

### `RequestCostBreakdown`

Suggested fields:

- `input_cost`
- `output_cost`
- `fixed_cost`
- `total_cost`
- `currency`
- `pricing_status`
- `pricing_context_json`

The calculator returns this object. Request logs persist it.

## Existing Table Ownership

### `model_base_prices`

Owns canonical model base prices. It answers "what is this model's base price?" It does not know station keys or group multipliers.

### `station_group_bindings`

Owns current group identity and binding metadata for a station. It should identify which remote group a local key or station group refers to.

### `group_rate_records`

Owns collected multiplier evidence and history. It should remain evidence/history, not a page-local projection.

### `station_keys`

Owns key identity, secret reference, routing participation, and current group binding pointer.

Existing fields such as `rate_multiplier`, `rate_source`, and `rate_collected_at` should remain as compatibility caches during migration. New billing code should prefer resolved group facts through the pricing resolver.

### `pricing_rules`

Owns manual pricing overrides. It should not be the general storage mechanism for collected group multipliers.

Any manual override used for request pricing must match the requested model or a deliberate model pattern. A rule for another model must never price the current request.

### `request_logs`

Owns immutable request-time cost snapshots. Existing estimated cost fields may be reused, but writes should come from the unified calculator. Read paths should not silently recompute old rows from current prices.

## Pricing Resolution Priority

The resolver should use this priority:

1. Manual override that exactly matches the station/key/group scope and requested model.
2. Key-bound group multiplier fact multiplied by model base price.
3. Current station/group multiplier fact multiplied by model base price.
4. Model base price with `1.0x`, returned as `base_price_only`.
5. `missing_model_price` or `unpriced` with a concrete reason.

Hard rules:

- A manual rule for a different model cannot price the current request.
- Missing manual rules do not mean unpriced if a model base price exists.
- Balance normalization values must not be used as group multipliers.
- Currency must be explicit.
- Currency totals must not be mixed.

## Data Flow

### Monitoring Request

1. The monitor sends a request with a known `station_key_id` and requested model.
2. The monitor calls the pricing resolver, or the shared monitoring backend path calls it before cost calculation.
3. The resolver loads station key, station, group binding, group rate fact, manual override, and model base price.
4. The resolver returns `ResolvedPricingContext`.
5. After usage is known, the monitor calls the cost calculator.
6. The monitor writes the request log with the pricing context and cost snapshot.
7. Dashboard and logs read the saved snapshot.

### Proxy Request

1. Route selection picks a station key.
2. Proxy runtime resolves pricing context for the selected key and requested model.
3. After upstream usage is known, proxy runtime calls the same calculator.
4. Proxy runtime writes the same request log snapshot shape as monitoring.

### Dashboard

1. Dashboard reads request log snapshots.
2. It groups total cost by currency.
3. It separately counts rows with `missing_model_price`, `unpriced`, `unsupported_billing_mode`, or `legacy_estimate`.
4. It does not recompute current costs from model base prices or group rates.

### Pricing Page

The pricing page remains an observation and diagnostic surface. It can show current model base prices, current group multipliers, manual overrides, and pricing gaps. It is not part of request cost calculation.

## Display Rules

Recommended user-facing states:

- `priced`: show normal cost.
- `base_price_only`: show cost with "基准价估算".
- `missing_rate`: show cost if `1.0x` fallback was used, with a warning.
- `missing_model_price`: show no numeric cost and name the missing model.
- `unpriced`: show "未定价".
- `unsupported_billing_mode`: show "计费模式暂不支持".
- `legacy_estimate`: show "历史估算".

Dashboard total rules:

- Sum only rows with a numeric cost.
- Group sums by currency.
- Do not combine USD and CNY into one number.
- Show unpriced or unsupported counts separately.
- Label legacy estimates separately from request-time snapshots.

## Error Reasons

Every incomplete context should include a machine-readable `reason`.

Initial reason set:

- `model_base_price_not_found`
- `group_binding_missing`
- `group_rate_not_collected`
- `manual_rule_model_mismatch`
- `manual_override_invalid`
- `currency_mismatch`
- `unsupported_billing_mode`
- `legacy_log_without_snapshot`
- `request_usage_missing`

These reasons should be stable enough for tests and UI labels.

## Migration Strategy

### Stage 1: Resolver And Calculator

- Add a backend pricing resolver.
- Add a backend cost calculator.
- Add focused Rust tests for resolution priority and formulas.
- Keep existing tables.
- Keep existing frontend layout mostly unchanged.

### Stage 2: Monitor And Proxy Adoption

- Move channel monitoring request cost calculation to the shared calculator.
- Move proxy runtime request cost calculation to the shared calculator.
- Write request-time pricing snapshots into request logs.
- Stop adding new duplicated formulas.

### Stage 3: Request Log Read Path

- Remove dynamic read-side cost backfill for rows that already have snapshots.
- Mark old rows without snapshots as `legacy_estimate`.
- Preserve compatibility for old databases.

### Stage 4: Dashboard And Logs UI

- Dashboard reads snapshot status and groups totals by currency.
- Logs display pricing source, multiplier source, and status.
- "未定价" appears only for true `unpriced`.

### Stage 5: Pricing Diagnostics

- Add or refine diagnostics that explain why a station key/model is priced, estimated, missing a multiplier, or missing a model base price.
- Keep station-specific adapter details out of billing domain names.

## Test Plan

### Rust Tests

Required cases:

- Base price exists and no manual override exists: result is not `unpriced`.
- Key-bound group multiplier applies to model base price.
- Manual override only applies when the requested model matches.
- A manual rule for another model is ignored.
- Missing model base price returns `missing_model_price`.
- Fixed price participates in the same calculator.
- Monitor and proxy cost paths produce the same breakdown for the same context and usage.
- Request log snapshot remains unchanged after current model base price or multiplier changes.
- Mixed-currency rows are not summed into one dashboard total.

### TypeScript And UI Tests

Required cases:

- Dashboard labels `base_price_only` distinctly from `unpriced`.
- Dashboard groups cost totals by currency.
- Logs show pricing status and source chain.
- Pricing page continues to show current group multiplier comparison.

### Migration Tests

Required cases:

- Existing request logs without snapshot fields are readable.
- Existing station key multiplier cache fields remain readable.
- Existing model base prices remain available.
- Existing manual pricing rules still work when model matching is exact.

## Acceptance Criteria

After the refactor, the question:

> Why did this station key request this model cost this amount?

must be answerable from one `ResolvedPricingContext.source_chain` and one `RequestCostBreakdown`.

The accepted end state:

- Proxy runtime, channel monitoring, request logs, and dashboard use the same backend pricing context.
- A request with a known model base price is not shown as true unpriced merely because no manual pricing rule exists.
- Group multipliers are resolved through group facts, not through page-local projections.
- Manual pricing overrides cannot leak across models.
- Fixed price and token price use one calculator.
- Request logs store request-time snapshots.
- Dashboard never mixes currencies in one total.
- New billing architecture code and tests use neutral domain naming.

## Implementation Notes

The first implementation plan should be test-driven. Start with failing resolver and calculator tests, then migrate one runtime path at a time.

Recommended file direction:

- `src-tauri/src/services/pricing/` owns resolver and calculator logic.
- `src-tauri/src/models/pricing.rs` may gain DTOs for resolved context and cost breakdown.
- `src-tauri/src/services/database.rs` should expose narrow database helpers, not host the domain algorithm forever.
- React and TypeScript code should consume backend outputs rather than rebuilding joins for request cost.

Keep compatibility caches until all consumers have migrated and a separate field-ownership audit approves removal.
