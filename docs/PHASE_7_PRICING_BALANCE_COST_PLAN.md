# P7 Pricing, Balance, and Cost Strategy Plan

## Goal

P7 upgrades Relay Pool Desktop from a capability and health router into a cost-aware router that can compare normalized prices, avoid low-balance stations, estimate request costs, and explain those decisions.

## Scope

P7 keeps `Station Key` as the routing object.

- `Station` owns the account, base URL, login state, and collector snapshots.
- `Station Key` owns enabled state, priority, capability scope, health, cooldown, and routing decisions.
- `Pricing Rule` owns normalized model / station / group price data.
- `Balance Snapshot` owns normalized balance or quota state with explicit units.
- `Request Cost` owns per-request usage and estimated cost metadata.

## Policies

P7 supports these routing policies:

- `priority_fallback`
- `stable_first`
- `backup_only`
- `cheap_first`

`cheap_first` is the first cost-aware policy. It should still respect capability, health, cooldown, and low-balance filters before it prefers a cheaper candidate.

## UI Targets

- Pricing page shows real `pricing_rules` data.
- Dashboard shows proxy status, known balance summary, and estimated cost summary.
- Logs page shows token and cost metadata.
- Routing page explains `cheap_first`, model aliases, and the reason each candidate was accepted or rejected.
- Key Pool keeps compact balance / cost summaries without turning into an accounting table.

## Non-Goals

P7 does not do:

- complex pricing DSL
- exact billing or invoicing
- auto recharge
- LAN exposure
- prompt / response persistence
- full financial reporting

## Completion Standard

P7 is complete when:

1. price data is visible from stored `pricing_rules`
2. balance data is visible from stored `balance_snapshots`
3. request logs show token / cost metadata
4. `cheap_first` is available in the router / simulator / UI
5. low or depleted balance can suppress or downgrade a candidate
6. dashboard, logs, routing, and pricing pages all use the real economic data
7. docs consistently describe the same product model
8. build and Rust tests pass

## Smoke Checklist

- `pnpm build`
- `cargo check --manifest-path .\src-tauri\Cargo.toml`
- `cargo test --manifest-path .\src-tauri\Cargo.toml --lib`
- open Pricing / Routing / Logs / Dashboard and confirm they show real data instead of mock-only copy
- confirm no prompt, response, or full API key is logged
