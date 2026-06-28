# Phase 6 Routing Policy and Model Capabilities

## Goal

P6 upgrades Relay Pool Desktop from a local gateway that mainly follows Key Pool priority into a model-aware, protocol-aware, health-aware Station Key router.

The route object remains `Station Key`, not `Station`.

## Completed Capabilities

- Durable key capability table: protocol flags, stream/tools/vision/reasoning flags, model allowlist, model blocklist, preferred models, backup-only flag, and routing tags.
- Global model alias table: client model name maps to upstream model name.
- Durable key health table: last success, last failure, consecutive failures, success/failure counts, average latency, last error, and cooldown expiration.
- Proxy runtime selector integration for `/v1/chat/completions` and `/v1/responses`.
- Model alias rewrite before upstream forwarding while request logs keep the client-facing model.
- Selector policies:
  - `priority_fallback`
  - `stable_first`
  - `backup_only`
- Cooldown rule:
  - 3 consecutive failures: 2 minutes
  - 4 consecutive failures: 5 minutes
  - 5 or more consecutive failures: 15 minutes
- Route simulator command and Routing Rules UI.
- Key Pool routing scope controls.
- Channel Status reads durable key health plus recent request logs.
- Request logs store route policy, selected reason, and rejected candidate reasons.

## Data Model

P6 adds:

- `station_key_capabilities`
- `model_aliases`
- `station_key_health`

P6 extends `request_logs` with:

- `route_policy`
- `route_reason`
- `rejected_candidates_json`

These fields store route metadata only. They must not contain prompt text, response text, full API keys, cookies, sessions, or tokens.

## Routing Behavior

For chat and responses requests, the proxy:

1. Parses endpoint kind, model, stream, tools, vision, and reasoning usage.
2. Loads enabled Station Key candidates.
3. Loads enabled model aliases.
4. Applies model alias mapping for upstream forwarding.
5. Filters candidates by protocol capability.
6. Filters candidates by stream/tools/vision/reasoning capability.
7. Filters candidates by model allowlist/blocklist.
8. Skips keys in cooldown.
9. Scores remaining keys by selected policy.
10. Forwards with fallback through the ordered candidates.
11. Updates key health after success or failure.
12. Writes request log metadata explaining the route decision.

`/v1/models` keeps the P5 aggregation behavior.

## UI Behavior

### Key Pool

Key Pool displays:

- protocol capability summary
- model scope summary
- backup-only badge
- cooldown badge
- success rate
- average latency
- consecutive failures
- recent error summary

The edit dialog can update:

- protocol capabilities
- model allowlist / blocklist / preferred models
- backup-only flag
- routing tags

### Routing Rules

Routing Rules is the main P6 control surface. It supports:

- default policy selection
- model alias CRUD
- route simulator
- accepted candidate ordering
- rejected candidate reasons

The simulator does not call upstream. It uses the same selector data as the runtime.

### Channel Status

Channel Status reads durable key health and recent request logs. It shows:

- success/failure counts
- success rate
- average latency
- consecutive failures
- cooldown state
- recent 60 request bars
- last error summary

### Request Logs

Request Logs show:

- route policy
- selected route reason
- rejected candidates count
- rejected candidate reasons

Logs still do not store prompt, response, full API key, cookie, session, or token material.

## Non-Goals

P6 does not implement:

- price-optimal routing
- balance-based avoidance
- full cost calculation
- automatic model price collection
- complex strategy DSL
- team or cloud sync
- secret encryption migration
- LAN exposure
- mid-stream fallback after bytes have been sent
- perfect automatic capability discovery for every upstream

## Manual Smoke Checklist

1. Open Key Pool and edit a key.
2. Disable `Responses`, add an allowlist model, mark backup-only, save, reopen, and confirm persistence.
3. Open Routing Rules.
4. Add alias `gpt-5.4 -> openai/gpt-5.4`.
5. Run simulator for `responses + gpt-5.4 + stream`.
6. Confirm selected key and rejected reasons are readable.
7. Trigger successful proxy request and confirm key health updates.
8. Trigger repeated failures against a bad upstream and confirm cooldown appears.
9. Confirm simulator and real proxy skip cooldown keys.
10. Open Request Logs and confirm route policy/reason are shown.
11. Confirm logs do not contain prompt, response body, or full API key.

## Known Limitations

- Capability data is user-configured; automatic discovery is partial future work.
- Model aliases are global, not per station or per key.
- `stable_first` uses simple durable health scoring, not a full statistical health engine.
- Cooldown windows are fixed.
- `/v1/models` still follows the P5 aggregation path.
- Pricing and balance signals are intentionally reserved for P7/P8.
